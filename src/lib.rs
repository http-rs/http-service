//! Types and traits giving an interface between low-level http server implementations
//! and services that use them. The interface is based on the `std::futures` API.
//!
//! ## Example
//! ```rust,no_run
//! use futures::{
//!     future::{self, BoxFuture, FutureExt},
//! };
//! use http_service::{HttpService, Response};
//! use std::net::{IpAddr, Ipv4Addr, SocketAddr};
//!
//! struct Server {
//!     message: Vec<u8>,
//! }
//!
//! impl Server {
//!     fn create(message: Vec<u8>) -> Server {
//!         Server {
//!             message,
//!         }
//!     }
//!
//!     pub async fn run(s: Server) {
//!         let a = SocketAddr::new(IpAddr::V4(Ipv4Addr::new(127, 0, 0, 1)), 8080);
//!         http_service_hyper::serve(s, a).await.unwrap()
//!     }
//! }
//!
//! impl HttpService for Server {
//!     type Connection = ();
//!     type ConnectionFuture = future::Ready<Result<(), std::io::Error>>;
//!     type ResponseFuture = BoxFuture<'static, Result<http_service::Response, std::io::Error>>;
//!
//!     fn connect(&self) -> Self::ConnectionFuture {
//!         future::ok(())
//!     }
//!
//!     fn respond(&self, _conn: &mut (), _req: http_service::Request) -> Self::ResponseFuture {
//!         let message = self.message.clone();
//!         async move { Ok(Response::new(http_service::Body::from(message))) }.boxed()
//!     }
//! }
//!
//! #[tokio::main]
//! async fn main() {
//!     let s = Server::create(String::from("Hello, World").into_bytes());
//!     Server::run(s).await;
//! }
//! ```

#![forbid(future_incompatible, rust_2018_idioms)]
#![deny(missing_debug_implementations, nonstandard_style)]
#![warn(missing_docs, missing_doc_code_examples)]
#![cfg_attr(any(feature = "nightly", test), feature(external_doc))]
#![cfg_attr(feature = "nightly", doc(include = "../README.md"))]
#![cfg_attr(test, deny(warnings))]

use bytes::Bytes;
use futures::{
    future,
    prelude::*,
    stream,
    task::{Context, Poll},
};

use std::fmt;
use std::pin::Pin;

#[cfg(test)]
#[doc(include = "../README.md")]
const _README: () = ();

/// The raw body of an http request or response.
///
/// A body is a stream of `Bytes` values, which are shared handles to byte buffers.
/// Both `Body` and `Bytes` values can be easily created from standard owned byte buffer types
/// like `Vec<u8>` or `String`, using the `From` trait.
pub struct Body {
    stream: Pin<Box<dyn Stream<Item = Result<Bytes, std::io::Error>> + Send + Sync + 'static>>,
}

impl Body {
    /// Create an empty body.
    pub fn empty() -> Self {
        Body::from_stream(stream::empty())
    }

    /// Create a body from a stream of `Bytes`
    pub fn from_stream<S>(s: S) -> Self
    where
        S: Stream<Item = Result<Bytes, std::io::Error>> + Send + Sync + 'static,
    {
        Self {
            stream: Box::pin(s),
        }
    }

    /// Reads the stream into a new `Vec`.
    #[allow(clippy::wrong_self_convention)] // https://github.com/rust-lang/rust-clippy/issues/4037
    pub async fn into_vec(mut self) -> std::io::Result<Vec<u8>> {
        let mut bytes = Vec::new();
        while let Some(chunk) = self.next().await {
            bytes.extend(chunk?);
        }
        Ok(bytes)
    }
}

impl<T: Into<Bytes> + Send> From<T> for Body {
    fn from(x: T) -> Self {
        Self::from_stream(stream::once(future::ok(x.into())))
    }
}

impl Unpin for Body {}

impl Stream for Body {
    type Item = Result<Bytes, std::io::Error>;
    fn poll_next(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<Self::Item>> {
        self.stream.poll_next_unpin(cx)
    }
}

impl fmt::Debug for Body {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Body").finish()
    }
}

/// An HTTP request with a streaming body.
pub type Request = http::Request<Body>;

/// An HTTP response with a streaming body.
pub type Response = http::Response<Body>;

/// An async HTTP service
///
/// An instance represents a service as a whole. The associated `Conn` type
/// represents a particular connection, and may carry connection-specific state.
pub trait HttpService: Send + Sync + 'static {
    /// An individual connection.
    ///
    /// This associated type is used to establish and hold any per-connection state
    /// needed by the service.
    type Connection: Send + 'static;

    /// A future for setting up an individual connection.
    ///
    /// This method is called each time the server receives a new connection request,
    /// but before actually exchanging any data with the client.
    ///
    /// Returning an error will result in the server immediately dropping
    /// the connection.
    type ConnectionFuture: Send + 'static + TryFuture<Ok = Self::Connection>;

    /// Initiate a new connection.
    ///
    /// This method is given access to the global service (`&self`), which may provide
    /// handles to connection pools, thread pools, or other global data.
    fn connect(&self) -> Self::ConnectionFuture;

    /// The async computation for producing the response.
    ///
    /// Returning an error will result in the server immediately dropping
    /// the connection. It is usually preferable to instead return an HTTP response
    /// with an error status code.
    type ResponseFuture: Send + 'static + TryFuture<Ok = Response>;

    /// Begin handling a single request.
    ///
    /// The handler is given shared access to the service itself, and mutable access
    /// to the state for the connection where the request is taking place.
    fn respond(&self, conn: &mut Self::Connection, req: Request) -> Self::ResponseFuture;
}

impl<F, R> HttpService for F
where
    F: Send + Sync + 'static + Fn(Request) -> R,
    R: Send + 'static + TryFuture<Ok = Response>,
    R::Error: Send,
{
    type Connection = ();
    type ConnectionFuture = future::Ready<Result<(), R::Error>>;
    fn connect(&self) -> Self::ConnectionFuture {
        future::ok(())
    }

    type ResponseFuture = R;
    fn respond(&self, _: &mut (), req: Request) -> Self::ResponseFuture {
        (self)(req)
    }
}
