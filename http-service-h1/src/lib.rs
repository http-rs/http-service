//! `HttpService` server that uses async-h1 as backend.

#![forbid(future_incompatible, rust_2018_idioms)]
#![deny(missing_debug_implementations, nonstandard_style)]
#![warn(missing_docs, missing_doc_code_examples)]
#![cfg_attr(test, deny(warnings))]

use std::future::Future;

use http_service::HttpService;

use async_std::net::{SocketAddr, TcpStream};
use async_std::prelude::*;
use async_std::stream::Stream;
use async_std::sync::Arc;
use async_std::{io, task};

/// A listening HTTP server that accepts connections in HTTP1.
#[derive(Debug)]
pub struct Server<I, S: HttpService> {
    incoming: I,
    inner: Arc<InnerServer<S>>,
}

#[derive(Debug)]
struct InnerServer<S: HttpService> {
    service: S,
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
    /// use http_service::{Response, Body, Error};
    /// use http_service_h1::Server;
    /// use async_std::net::TcpListener;
    ///
    /// // And an HttpService to handle each connection...
    /// let service = |req| async {
    ///     Ok::<Response, Error>(Response::from("Hello World"))
    /// };
    ///
    /// async_std::task::block_on(async move {
    ///     // Then bind, configure the spawner to our pool, and serve...
    ///     let mut listener = TcpListener::bind("127.0.0.1:3000").await?;
    ///     let addr = format!("http://{}", listener.local_addr()?);
    ///     let mut server = Server::new(addr, listener.incoming(), service);
    ///     server.run().await?;
    ///     Ok::<(), Error>(())
    /// })?;
    /// # Ok::<(), Error>(())
    pub fn new(addr: String, incoming: I, service: S) -> Self {
        Server {
            incoming,
            inner: Arc::new(InnerServer { service, addr }),
        }
    }

    /// Run the server forever-ish.
    pub async fn run(&mut self) -> io::Result<()> {
        while let Some(stream) = self.incoming.next().await {
            let server = self.inner.clone();

            task::spawn(async move {
                let stream = match stream {
                    Ok(stream) => stream,
                    Err(err) => {
                        log::warn!("failed to establish connection: {}", err);
                        return;
                    }
                };

                let res = async_h1::accept(&server.addr, stream.clone(), |req| async {
                    let conn = server.service.connect().await.map_err(Into::into)?;
                    server.service.respond(conn, req).await.map_err(Into::into)
                })
                .await;

                if let Err(err) = res {
                    log::error!("{}", err);
                }
            });
        }

        Ok(())
    }
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
