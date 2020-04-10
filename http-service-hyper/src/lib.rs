//! `HttpService` server that uses Hyper as backend.

#![forbid(future_incompatible, rust_2018_idioms)]
#![deny(missing_debug_implementations, nonstandard_style)]
#![warn(missing_docs, missing_doc_code_examples)]
#![cfg_attr(test, deny(warnings))]

#[cfg(feature = "runtime")]
use futures::prelude::*;
use futures::task::Spawn;
use http_service::{Body, HttpService};
use hyper::server::{Builder as HyperBuilder, Server as HyperServer};

use std::convert::TryInto;
#[cfg(feature = "runtime")]
use std::net::SocketAddr;
use std::pin::Pin;
use std::sync::Arc;
use std::task::{self, Poll};

// Wrapper type to allow us to provide a blanket `Service` impl
struct WrapConnection<H: HttpService> {
    service: Arc<H>,
    connection: H::Connection,
}

impl<H> hyper::service::Service<hyper::Body> for WrapConnection<H>
where
    H: HttpService,
{
    type Response = http::Response<hyper::Body>;
    type Error = std::io::Error;
    type Future = Pin<Box<dyn Future<Output = Result<Self::Response, Self::Error>>>>;

    fn call(&mut self, req: http::Request<hyper::Body>) -> Self::Future {
        // Convert Request
        let error = std::io::Error::from(std::io::ErrorKind::Other);
        let req_hyper: http::Request<Body> = req.map(|body| {
            use futures::stream::TryStreamExt;
            let body_stream = body.map(|chunk| chunk.map(|c| c.to_vec()).map_err(|_| error));
            let body_reader = body_stream.into_async_read();
            Body::from_reader(body_reader, None)
        });

        let req: http_types::Request = req_hyper.try_into().unwrap();
        let fut = self.service.respond(self.connection, req);

        // Convert Request
        let fut = async {
            let res: http_types::Response = fut.into_future().await.map_err(|_| error)?;
            let res_hyper = hyper::Response::<Body>::from(res);

            let (parts, body) = res_hyper.into_parts();
            let body = hyper::Body::wrap_stream(body);

            Ok(hyper::Response::from_parts(parts, body))
        };

        Box::pin(fut)
    }
}

/// A listening HTTP server that accepts connections in both HTTP1 and HTTP2 by default.
///
/// [`Server`] is a [`Future`] mapping a bound listener with a set of service handlers. It is built
/// using the [`Builder`], and the future completes when the server has been shutdown. It should be
/// run by an executor.
#[allow(clippy::type_complexity)] // single-use type with many compat layers
pub struct Server<I: TryStream, S, Sp> {
    inner: HyperServer<I, S, Sp>,
}

impl<I: TryStream, S, Sp> std::fmt::Debug for Server<I, S, Sp> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Server").finish()
    }
}

/// A builder for a [`Server`].
#[allow(clippy::type_complexity)] // single-use type with many compat layers
pub struct Builder<I: TryStream, Sp> {
    inner: HyperBuilder<I, Sp>,
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
            inner: HyperServer::builder(incoming),
        }
    }
}

impl<I: TryStream, Sp> Builder<I, Sp> {
    /// Sets the [`Spawn`] to deal with starting connection tasks.
    pub fn with_spawner<Sp2>(self, new_spawner: Sp2) -> Builder<I, Sp2> {
        Builder {
            inner: self.inner.executor(new_spawner),
        }
    }

    /// Consume this [`Builder`], creating a [`Server`].
    ///
    /// # Examples
    ///
    /// ```no_run
    /// use http_service::{Response, Body};
    /// use http_service_hyper::Server;
    /// use async_std::net::TcpListener;
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
        Sp: Clone + Send + 'static,
        for<'a> &'a Sp: Spawn,
    {
        Server {
            inner: self.inner.serve(service),
        }
    }
}

impl<I, S, Sp> Future for Server<I, S, Sp>
where
    I: TryStream + Unpin,
    I::Ok: AsyncRead + AsyncWrite + Send + Unpin + 'static,
    I::Error: Into<Box<dyn std::error::Error + Send + Sync>>,
    S: HttpService,
    Sp: Clone + Send + 'static,
    for<'a> &'a Sp: Spawn,
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
    hyper::Server::bind(&addr).serve(s).compat()
}

/// Run the given `HttpService` at the given address on the default runtime, using `hyper` as
/// backend.
#[cfg(feature = "runtime")]
pub fn run<S: HttpService>(s: S, addr: SocketAddr) {
    let server = serve(s, addr).map(|_| Result::<_, ()>::Ok(())).compat();
    hyper::rt::Executor::execute(server);
}
