//! Types and traits giving an interface between low-level http server implementations
//! and services that use them. The interface is based on the `std::futures` API.

#![warn(missing_debug_implementations, rust_2018_idioms)]
#![allow(clippy::mutex_atomic, clippy::module_inception)]
#![doc(test(attr(deny(rust_2018_idioms, warnings))))]
#![doc(test(attr(allow(unused_extern_crates, unused_variables))))]

use async_std::io::{self, prelude::*};
use async_std::task::{Context, Poll};

use futures::future::TryFuture;

use std::fmt;
use std::pin::Pin;

pin_project_lite::pin_project! {
    /// The raw body of an http request or response.
    pub struct Body {
        #[pin]
        reader: Box<dyn BufRead + Unpin + Send + 'static>,
    }
}

impl Body {
    /// Create a new empty body.
    pub fn empty() -> Self {
        Self {
            reader: Box::new(io::empty()),
        }
    }

    /// Create a new instance from a reader.
    pub fn from_reader(reader: impl BufRead + Unpin + Send + 'static) -> Self {
        Self {
            reader: Box::new(reader),
        }
    }
}

impl Read for Body {
    fn poll_read(
        mut self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        buf: &mut [u8],
    ) -> Poll<io::Result<usize>> {
        Pin::new(&mut self.reader).poll_read(cx, buf)
    }
}

impl BufRead for Body {
    fn poll_fill_buf(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<io::Result<&'_ [u8]>> {
        let this = self.project();
        this.reader.poll_fill_buf(cx)
    }

    fn consume(mut self: Pin<&mut Self>, amt: usize) {
        Pin::new(&mut self.reader).consume(amt)
    }
}

impl fmt::Debug for Body {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Body").field("reader", &"<hidden>").finish()
    }
}

impl From<Vec<u8>> for Body {
    fn from(vec: Vec<u8>) -> Body {
        Self {
            reader: Box::new(io::Cursor::new(vec)),
        }
    }
}

impl<R: BufRead + Unpin + Send + 'static> From<Box<R>> for Body {
    /// Converts an `AsyncRead` into a Body.
    fn from(reader: Box<R>) -> Self {
        Self { reader }
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

// impl<F, R, E> HttpService<E> for F
// where
//     F: Send + Sync + 'static + Fn(Request) -> R,
//     R: Send + 'static + Future<Output = Result<Response, E>>,
// {
//     type ResponseFuture = R;
//     fn respond(&self, req: Request) -> Self::ResponseFuture {
//         (self)(req)
//     }
// }
