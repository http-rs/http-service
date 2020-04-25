//! `HttpService` server that uses async-h1 as backend.

#![forbid(future_incompatible, rust_2018_idioms)]
#![deny(missing_debug_implementations, nonstandard_style)]
#![warn(missing_docs, missing_doc_code_examples)]
#![cfg_attr(test, deny(warnings))]

use std::future::Future;
use std::pin::Pin;
use std::task::{Context, Poll};

use http_service::{Error, HttpService};

use async_std::io;
use async_std::net::{SocketAddr, TcpStream};
use async_std::prelude::*;
use async_std::stream::Stream;
use async_std::sync::Arc;

/// A listening HTTP server that accepts connections in HTTP1.
#[derive(Debug)]
pub struct Server<I, S: HttpService> {
    incoming: I,
    service: Arc<S>,
    addr: String,
}

impl<I: Stream<Item = io::Result<TcpStream>>, S: HttpService> Server<I, S>
where
    <<S as HttpService>::ResponseFuture as Future>::Output: Send,
    <S as HttpService>::Connection: Sync,
    I: Unpin + Send + Sync,
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
    ///     let mut server = Server::new(addr, listener.incoming(), service);
    ///     server.run().await?;
    ///     Ok::<(), Box<dyn std::error::Error>>(())
    /// })?;
    /// # Ok::<(), Box<dyn std::error::Error>>(())
    pub fn new(addr: String, incoming: I, service: S) -> Self {
        Server {
            service: Arc::new(service),
            incoming,
            addr,
        }
    }

    /// Run the server forever-ish.
    pub async fn run(&mut self) -> io::Result<()> {
        while let Some(stream) = self.incoming.next().await {
            let stream = stream?;
            async_std::task::spawn(accept(self.addr.clone(), self.service.clone(), stream));
        }

        Ok(())
    }
}

/// Accept a new connection.
async fn accept<S>(addr: String, service: Arc<S>, stream: TcpStream) -> Result<(), Error>
where
    S: HttpService,
    <<S as HttpService>::ResponseFuture as Future>::Output: Send,
    <S as HttpService>::Connection: Sync,
{
    // TODO: Delete this line when we implement `Clone` for `TcpStream`.
    let stream = WrapStream(Arc::new(stream));

    let conn = service
        .clone()
        .connect()
        .await
        .map_err(|_| io::Error::from(io::ErrorKind::Other))?;

    async_h1::accept(&addr, stream.clone(), |req| async {
        let conn = conn.clone();
        let service = service.clone();
        req.peer_addr = stream.0.peer_addr().map(|socket| socket.to_string()).ok();
        req.local_addr = stream.0.local_addr().map(|socket| socket.to_string()).ok();

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
    let addr = format!("http://{}", listener.local_addr()?); // TODO: https
    let mut server = Server::<_, S>::new(addr, listener.incoming(), service);

    server.run().await
}

#[derive(Clone)]
struct WrapStream(Arc<TcpStream>);

impl io::Read for WrapStream {
    fn poll_read(
        self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        buf: &mut [u8],
    ) -> Poll<io::Result<usize>> {
        Pin::new(&mut &*self.0).poll_read(cx, buf)
    }
}

impl io::Write for WrapStream {
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
