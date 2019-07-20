//! `HttpService` server that uses Hyper as backend.

#![forbid(future_incompatible, rust_2018_idioms)]
#![deny(missing_debug_implementations, nonstandard_style)]
#![warn(missing_docs, missing_doc_code_examples)]
#![cfg_attr(test, deny(warnings))]

use futures::{future::BoxFuture, prelude::*, stream, task::Spawn};
use futures_tokio_compat::Compat;
use http_service::{Body, HttpService};
use hyper::server::{Builder as HyperBuilder, Server as HyperServer};
#[cfg(feature = "runtime")]
use std::net::SocketAddr;
use std::{
    pin::Pin,
    sync::Arc,
    task::{self, Poll},
};

// Wrapper type to allow us to provide a blanket `MakeService` impl
struct WrapHttpService<H> {
    service: Arc<H>,
}

// Wrapper type to allow us to provide a blanket `Service` impl
struct WrapConnection<H: HttpService> {
    service: Arc<H>,
    connection: H::Connection,
}

impl<H, Ctx> hyper::service::MakeService<Ctx> for WrapHttpService<H>
where
    H: HttpService,
{
    type ReqBody = hyper::Body;
    type ResBody = hyper::Body;
    type Error = std::io::Error;
    type Service = WrapConnection<H>;
    type Future = BoxFuture<'static, Result<Self::Service, Self::Error>>;
    type MakeError = std::io::Error;

    fn make_service(&mut self, _ctx: Ctx) -> Self::Future {
        let service = self.service.clone();
        let error = std::io::Error::from(std::io::ErrorKind::Other);
        async move {
            let connection = service.connect().into_future().await.map_err(|_| error)?;
            Ok(WrapConnection {
                service,
                connection,
            })
        }
            .boxed()
    }
}

impl<H> hyper::service::Service for WrapConnection<H>
where
    H: HttpService,
{
    type ReqBody = hyper::Body;
    type ResBody = hyper::Body;
    type Error = std::io::Error;
    type Future = BoxFuture<'static, Result<http::Response<hyper::Body>, Self::Error>>;

    fn call(&mut self, req: http::Request<hyper::Body>) -> Self::Future {
        let error = std::io::Error::from(std::io::ErrorKind::Other);
        let req = req.map(|hyper_body| {
            let stream = hyper_body.map(|c| match c {
                Ok(chunk) => Ok(chunk.into_bytes()),
                Err(e) => Err(std::io::Error::new(std::io::ErrorKind::Other, e)),
            });
            Body::from_stream(stream)
        });
        let fut = self.service.respond(&mut self.connection, req);

        async move {
            let res: http::Response<_> = fut.into_future().await.map_err(|_| error)?;
            Ok(res.map(hyper::Body::wrap_stream))
        }
            .boxed()
    }
}

/// A listening HTTP server that accepts connections in both HTTP1 and HTTP2 by default.
///
/// [`Server`] is a [`Future`] mapping a bound listener with a set of service handlers. It is built
/// using the [`Builder`], and the future completes when the server has been shutdown. It should be
/// run by an executor.
#[allow(clippy::type_complexity)] // single-use type with many compat layers
pub struct Server<I: TryStream, S, Sp> {
    inner:
        HyperServer<stream::MapOk<I, fn(I::Ok) -> Compat<I::Ok>>, WrapHttpService<S>, Compat<Sp>>,
}

impl<I: TryStream, S, Sp> std::fmt::Debug for Server<I, S, Sp> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Server").finish()
    }
}

/// A builder for a [`Server`].
#[allow(clippy::type_complexity)] // single-use type with many compat layers
pub struct Builder<I: TryStream, Sp> {
    inner: HyperBuilder<stream::MapOk<I, fn(I::Ok) -> Compat<I::Ok>>, Compat<Sp>>,
}

impl<I: TryStream, Sp> std::fmt::Debug for Builder<I, Sp> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Builder").finish()
    }
}

impl<I: TryStream> Server<I, (), ()> {
    /// Starts a [`Builder`] with the provided incoming stream.
    pub fn builder(incoming: I) -> Builder<I, ()> {
        Builder {
            inner: HyperServer::builder(incoming.map_ok(Compat::new as _))
                .executor(Compat::new(())),
        }
    }
}

impl<I: TryStream, Sp> Builder<I, Sp> {
    /// Sets the [`Spawn`] to deal with starting connection tasks.
    pub fn with_spawner<Sp2>(self, new_spawner: Sp2) -> Builder<I, Sp2> {
        Builder {
            inner: self.inner.executor(Compat::new(new_spawner)),
        }
    }

    /// Consume this [`Builder`], creating a [`Server`].
    ///
    /// # Examples
    ///
    /// ```no_run
    /// use http_service::{Response, Body};
    /// use http_service_hyper::Server;
    /// use romio::TcpListener;
    ///
    /// // Construct an executor to run our tasks on
    /// let mut pool = futures::executor::ThreadPool::new()?;
    ///
    /// // And an HttpService to handle each connection...
    /// let service = |req| {
    ///     futures::future::ok::<_, ()>(Response::new(Body::from("Hello World")))
    /// };
    ///
    /// // Then bind, configure the spawner to our pool, and serve...
    /// let addr = "127.0.0.1:3000".parse()?;
    /// let mut listener = TcpListener::bind(&addr)?;
    /// let server = Server::builder(listener.incoming())
    ///     .with_spawner(pool.clone())
    ///     .serve(service);
    ///
    /// // Finally, spawn `server` onto our executor...
    /// pool.run(server)?;
    /// # Ok::<(), Box<dyn std::error::Error>>(())
    pub fn serve<S: HttpService>(self, service: S) -> Server<I, S, Sp>
    where
        I: TryStream + Unpin,
        I::Ok: AsyncRead + AsyncWrite + Send + Unpin + 'static,
        I::Error: Into<Box<dyn std::error::Error + Send + Sync>>,
        Sp: Clone + Spawn + Unpin + Send + 'static,
    {
        Server {
            inner: self.inner.serve(WrapHttpService {
                service: Arc::new(service),
            }),
        }
    }
}

impl<I, S, Sp> Future for Server<I, S, Sp>
where
    I: TryStream + Unpin,
    I::Ok: AsyncRead + AsyncWrite + Send + Unpin + 'static,
    I::Error: Into<Box<dyn std::error::Error + Send + Sync>>,
    S: HttpService,
    Sp: Clone + Spawn + Unpin + Send + 'static,
{
    type Output = hyper::Result<()>;

    fn poll(mut self: Pin<&mut Self>, cx: &mut task::Context<'_>) -> Poll<hyper::Result<()>> {
        self.inner.poll_unpin(cx)
    }
}

/// Serve the given `HttpService` at the given address, using `hyper` as backend, and return a
/// `Future` that can be `await`ed on.
#[cfg(feature = "runtime")]
pub fn serve<S: HttpService>(
    s: S,
    addr: SocketAddr,
) -> impl Future<Output = Result<(), hyper::Error>> {
    let service = WrapHttpService {
        service: Arc::new(s),
    };
    hyper::Server::bind(&addr).serve(service)
}
