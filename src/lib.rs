//! Types and traits giving an interface between low-level http server implementations
//! and services that use them. The interface is based on the `std::futures` API.

#![warn(missing_debug_implementations, rust_2018_idioms)]
#![allow(clippy::mutex_atomic, clippy::module_inception)]
#![doc(test(attr(deny(rust_2018_idioms, warnings))))]
#![doc(test(attr(allow(unused_extern_crates, unused_variables))))]

use std::future::Future;
use std::pin::Pin;
use std::task::{Context, Poll};

/// The raw body of an http request or response.
pub type Body = http_types::Body;

/// An HTTP request with a streaming body.
pub type Request = http_types::Request;

/// An HTTP response with a streaming body.
pub type Response = http_types::Response;

/// An HTTP compatible error type.
pub type Error = http_types::Error;

/// An async HTTP service
///
/// An instance represents a service as a whole. The associated `Conn` type
/// represents a particular connection, and may carry connection-specific state.
pub trait HttpService: Send + Sync + 'static {
    /// An individual connection.
    ///
    /// This associated type is used to establish and hold any per-connection state
    /// needed by the service.
    type Connection: Send + 'static + Clone;

    /// Error when trying to connect.
    type ConnectionError: Into<Error> + Send;

    /// A future for setting up an individual connection.
    ///
    /// This method is called each time the server receives a new connection request,
    /// but before actually exchanging any data with the client.
    ///
    /// Returning an error will result in the server immediately dropping
    /// the connection.
    type ConnectionFuture: Send
        + 'static
        + Future<Output = Result<Self::Connection, Self::ConnectionError>>;

    /// Initiate a new connection.
    ///
    /// This method is given access to the global service (`&self`), which may provide
    /// handles to connection pools, thread pools, or other global data.
    fn connect(&self) -> Self::ConnectionFuture;

    /// Response error.
    type ResponseError: Into<Error> + Send;

    /// The async computation for producing the response.
    ///
    /// Returning an error will result in the server immediately dropping
    /// the connection. It is usually preferable to instead return an HTTP response
    /// with an error status code.
    type ResponseFuture: Send + 'static + Future<Output = Result<Response, Self::ResponseError>>;

    /// Begin handling a single request.
    ///
    /// The handler is given shared access to the service itself, and mutable access
    /// to the state for the connection where the request is taking place.
    fn respond(&self, conn: Self::Connection, req: Request) -> Self::ResponseFuture;
}

impl<F, R, E> HttpService for F
where
    F: Send + Sync + 'static + Fn(Request) -> R,
    R: Send + 'static + Future<Output = Result<Response, E>>,
    E: Send + Into<Error>,
{
    type Connection = ();
    type ConnectionError = Error;
    type ConnectionFuture = OkFuture;
    type ResponseFuture = R;
    type ResponseError = E;

    fn connect(&self) -> Self::ConnectionFuture {
        OkFuture(true)
    }

    fn respond(&self, _conn: Self::Connection, req: Request) -> Self::ResponseFuture {
        (self)(req)
    }
}

/// A future which resolves to `Ok(())`.
#[derive(Debug)]
pub struct OkFuture(bool);

impl Unpin for OkFuture {}

impl Future for OkFuture {
    type Output = Result<(), Error>;

    #[inline]
    fn poll(mut self: Pin<&mut Self>, _cx: &mut Context<'_>) -> Poll<Self::Output> {
        self.0 = false;
        Poll::Ready(Ok(()))
    }
}
