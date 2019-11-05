//! `HttpService` server that uses Hyper as backend.

#![forbid(future_incompatible, rust_2018_idioms)]
#![deny(missing_debug_implementations, nonstandard_style)]
#![warn(missing_docs, missing_doc_code_examples)]
#![cfg_attr(test, deny(warnings))]

#[cfg(feature = "runtime")]
use futures::compat::{Compat as Compat03As01, Compat01As03};
use futures::compat::Future01CompatExt;
use futures::future::BoxFuture;
use futures::prelude::*;
use futures::stream;
use futures::task::Spawn;
use http_service::{Body, HttpService};
use hyper::server::{Builder as HyperBuilder, Server as HyperServer};
use std::marker::PhantomData;

#[cfg(feature = "runtime")]
use std::net::SocketAddr;
use std::pin::Pin;
use std::sync::Arc;
use std::task::{self, Context, Poll};
use std::io;

// Wrapper type to allow us to provide a blanket `MakeService` impl
struct WrapHttpService<H> {
    service: Arc<H>,
}

// Wrapper type to allow us to provide a blanket `Service` impl
struct WrapConnection<H: HttpService<E1, E2>, E1, E2> {
    service: Arc<H>,
    connection: H::Connection,
}

impl<H, Ctx, E1, E2> hyper::service::MakeService<Ctx> for WrapHttpService<H, E1, E2>
where
    H: HttpService<E1, E2>,
{
    type ReqBody = hyper::Body;
    type ResBody = hyper::Body;
    type Error = std::io::Error;
    type Service = WrapConnection<H, E1, E2>;
    type Future = Compat03As01<BoxFuture<'static, Result<Self::Service, Self::Error>>>;
    type MakeError = std::io::Error;

    fn make_service(&mut self, _ctx: Ctx) -> Self::Future {
        let service = self.service.clone();
        let error = std::io::Error::from(std::io::ErrorKind::Other);
        async move {
            let connection = service.connect().into_future().await.map_err(|_| error)?;
            Ok(WrapConnection {
                service,
                connection,
                __error_1: PhantomData,
                __error_2: PhantomData,
            })
        }
        .boxed()
        .compat()
    }
}

impl<H, E1, E2> hyper::service::Service for WrapConnection<H, E1, E2>
where
    H: HttpService<E1, E2>,
{
    type ReqBody = hyper::Body;
    type ResBody = hyper::Body;
    type Error = std::io::Error;
    type Future = Compat03As01<BoxFuture<'static, Result<http::Response<hyper::Body>, Self::Error>>>;

    fn call(&mut self, req: http::Request<hyper::Body>) -> Self::Future {
        // Convert Request
        let error = std::io::Error::from(std::io::ErrorKind::Other);
        let req = req.map(|body| {
            let body_stream = Compat01As03::new(body)
                .map(|chunk| chunk.map(|chunk| chunk.to_vec()))
                .map_err(|e| io::Error::new(io::ErrorKind::Other, e));
            let body_reader = body_stream.into_async_read();
            Body::from_reader(Box::new(body_reader))
        });

        let fut = self.service.respond(self.connection, req);

        // Convert Request
        async move {
            let res: http::Response<_> = fut.into_future().await.map_err(|_| error)?;

            let (parts, body) = res.into_parts();
            let byte_stream = Compat03As01::new(ChunkStream { reader: body });
            let body = hyper::Body::wrap_stream(byte_stream);
            let res = hyper::Response::from_parts(parts, body);
            Ok(res)
        }.boxed().compat()
    }
}

/// A listening HTTP server that accepts connections in both HTTP1 and HTTP2 by default.
///
/// [`Server`] is a [`Future`] mapping a bound listener with a set of service handlers. It is built
/// using the [`Builder`], and the future completes when the server has been shutdown. It should be
/// run by an executor.
#[allow(clippy::type_complexity)] // single-use type with many compat layers
pub struct Server<I: TryStream, S, Sp> {
    inner: Compat01As03<
        HyperServer<
            Compat03As01<stream::MapOk<I, fn(I::Ok) -> Compat03As01<I::Ok>>>,
            WrapHttpService<S>,
            Compat03As01<Sp>,
        >,
    >,
}

impl<I: TryStream, S, Sp> std::fmt::Debug for Server<I, S, Sp> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Server").finish()
    }
}

/// A builder for a [`Server`].
#[allow(clippy::type_complexity)] // single-use type with many compat layers
pub struct Builder<I: TryStream, Sp> {
    inner: HyperBuilder<Compat03As01<stream::MapOk<I, fn(I::Ok) -> Compat03As01<I::Ok>>>, Compat03As01<Sp>>,
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
            inner: HyperServer::builder(Compat03As01::new(incoming.map_ok(Compat03As01::new as _)))
                .executor(Compat03As01::new(())),
        }
    }
}

impl<I: TryStream, Sp> Builder<I, Sp> {
    /// Sets the [`Spawn`] to deal with starting connection tasks.
    pub fn with_spawner<Sp2>(self, new_spawner: Sp2) -> Builder<I, Sp2> {
        Builder {
            inner: self.inner.executor(Compat03As01::new(new_spawner)),
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
    pub fn serve<S: HttpService<E1, E2>, E1, E2>(self, service: S) -> Server<I, S, Sp>
    where
        I: TryStream + Unpin,
        I::Ok: AsyncRead + AsyncWrite + Send + Unpin + 'static,
        I::Error: Into<Box<dyn std::error::Error + Send + Sync>>,
        Sp: Clone + Send + 'static,
        for<'a> &'a Sp: Spawn,
    {
        Server {
            inner: Compat01As03::new(self.inner.serve(WrapHttpService {
                service: Arc::new(service),
            })),
        }
    }
}

impl<I, S, Sp, E1, E2> Future for Server<I, S, Sp>
where
    I: TryStream + Unpin,
    I::Ok: AsyncRead + AsyncWrite + Send + Unpin + 'static,
    I::Error: Into<Box<dyn std::error::Error + Send + Sync>>,
    S: HttpService<E1, E2>,
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
pub fn serve<S: HttpService<E1, E2>, E1, E2>(
    s: S,
    addr: SocketAddr,
) -> impl Future<Output = Result<(), hyper::Error>> {
    let service = WrapHttpService {
        service: Arc::new(s),
    };
    hyper::Server::bind(&addr).serve(service).compat()
}

/// Run the given `HttpService` at the given address on the default runtime, using `hyper` as
/// backend.
#[cfg(feature = "runtime")]
pub fn run<S: HttpService<E1, E2>, E1, E2>(s: S, addr: SocketAddr) {
    let server = serve(s, addr).map(|_| Result::<_, ()>::Ok(())).compat();
    hyper::rt::run(server);
}

/// A type that wraps an `AsyncRead` into a `Stream` of `hyper::Chunk`. Used for writing data to a
/// Hyper response.
struct ChunkStream<R: AsyncRead> {
    reader: R,
}

impl<R: AsyncRead + Unpin> futures::Stream for ChunkStream<R> {
    type Item = Result<hyper::Chunk, Box<dyn std::error::Error + Send + Sync + 'static>>;

    fn poll_next(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<Self::Item>> {
        // This is not at all efficient, but that's okay for now.
        let mut buf = vec![];
        let read = futures::ready!(Pin::new(&mut self.reader).poll_read(cx, &mut buf))?;
        if read == 0 {
            return Poll::Ready(None);
        } else {
            buf.shrink_to_fit();
            let chunk = hyper::Chunk::from(buf);
            Poll::Ready(Some(Ok(chunk)))
        }
    }
}
