//! `HttpService` server that uses async-h1 as backend.

#![forbid(future_incompatible, rust_2018_idioms)]
#![deny(missing_debug_implementations, nonstandard_style)]
#![warn(missing_docs, missing_doc_code_examples)]
#![cfg_attr(test, deny(warnings))]

use std::future::Future;

use http_service::{Error, HttpService};

use async_std::io::{self, Read, Write};
use async_std::net::SocketAddr;
use async_std::prelude::*;
use async_std::stream::Stream;
use async_std::sync::Arc;
use async_std::task::{Context, Poll};
use std::pin::Pin;

/// A listening HTTP server that accepts connections in HTTP1.
#[derive(Debug)]
pub struct Server<I, S: HttpService> {
    incoming: I,
    service: Arc<S>,
}

impl<I, RW, S> Server<I, S>
where
    S: HttpService,
    <<S as HttpService>::ResponseFuture as Future>::Output: Send,
    <S as HttpService>::Connection: Sync,
    RW: Read + Write + Clone + Unpin + Send + Sync + 'static,
    I: Stream<Item = io::Result<RW>> + Unpin + Send + Sync,
{
    /// Consume this [`Builder`], creating a [`Server`].
    ///
    /// # Examples
    ///
    /// ```no_run
    /// use http_service::{Response, Body};
    /// use http_service_h1::Server;
    /// use async_std::net::TcpListener;
    ///
    /// // And an HttpService to handle each connection...
    /// let service = |req| async {
    ///     Ok::<Response, async_std::io::Error>(Response::from("Hello World"))
    /// };
    ///
    /// async_std::task::block_on(async move {
    ///     // Then bind, configure the spawner to our pool, and serve...
    ///     let mut listener = TcpListener::bind("127.0.0.1:3000").await?;
    ///     let addr = format!("http://{}", listener.local_addr()?);
    ///     let mut server = Server::new(listener.incoming(), service);
    ///     server.run().await?;
    ///     Ok::<(), Box<dyn std::error::Error>>(())
    /// })?;
    /// # Ok::<(), Box<dyn std::error::Error>>(())
    pub fn new(incoming: I, service: S) -> Self {
        Server {
            service: Arc::new(service),
            incoming,
        }
    }

    /// Run the server forever-ish.
    pub async fn run(&mut self) -> io::Result<()> {
        while let Some(read_write) = self.incoming.next().await {
            let read_write = read_write?;
            async_std::task::spawn(accept(self.service.clone(), read_write));
        }

        Ok(())
    }
}

/// Accept a new connection.
async fn accept<S, RW>(service: Arc<S>, read_write: RW) -> Result<(), Error>
where
    S: HttpService,
    <<S as HttpService>::ResponseFuture as Future>::Output: Send,
    <S as HttpService>::Connection: Sync,
    RW: Read + Write + Unpin + Clone + Send + Sync + 'static,
{
    let conn = service
        .clone()
        .connect()
        .await
        .map_err(|_| io::Error::from(io::ErrorKind::Other))?;

    async_h1::accept(read_write, |req| async {
        let conn = conn.clone();
        let service = service.clone();
        async move {
            let res = service
                .respond(conn, req)
                .await
                .map_err(|_| io::Error::from(io::ErrorKind::Other))?;
            Ok(res)
        }
        .await
    })
    .await?;

    Ok(())
}

/// Serve the given `HttpService` at the given address, using `async-h1` as backend, and return a
/// `Future` that can be `await`ed on.
pub async fn serve<S: HttpService>(service: S, addr: SocketAddr) -> io::Result<()>
where
    <<S as HttpService>::ResponseFuture as Future>::Output: Send,
    <S as HttpService>::Connection: Sync,
{
    let listener = async_std::net::TcpListener::bind(addr).await?;
    let mut server = Server::<_, S>::new(listener.incoming(), service);

    server.run().await
}

#[derive(Clone, Debug)]
#[allow(missing_docs)]
pub struct UnixStreamWrapper(Arc<async_std::os::unix::net::UnixStream>);

impl UnixStreamWrapper {
    #[allow(missing_docs)]
    pub fn stream_from_incoming<'a>(
        incoming: async_std::os::unix::net::Incoming<'a>,
    ) -> impl Stream<Item = io::Result<UnixStreamWrapper>> + 'a {
        incoming.map(|r| r.map(|io| UnixStreamWrapper(Arc::new(io))))
    }
}

impl async_std::io::Read for UnixStreamWrapper {
    fn poll_read(
        self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        buf: &mut [u8],
    ) -> Poll<io::Result<usize>> {
        Pin::new(&mut &*self.0).poll_read(cx, buf)
    }
}

impl io::Write for UnixStreamWrapper {
    fn poll_write(
        self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        buf: &[u8],
    ) -> Poll<io::Result<usize>> {
        Pin::new(&mut &*self.0).poll_write(cx, buf)
    }

    fn poll_flush(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<io::Result<()>> {
        Pin::new(&mut &*self.0).poll_flush(cx)
    }

    fn poll_close(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<io::Result<()>> {
        Pin::new(&mut &*self.0).poll_close(cx)
    }
}
