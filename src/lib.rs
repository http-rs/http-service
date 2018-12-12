#![cfg_attr(feature = "nightly", deny(missing_docs))]
#![cfg_attr(feature = "nightly", feature(external_doc))]
#![cfg_attr(feature = "nightly", doc(include = "../README.md"))]
#![cfg_attr(test, deny(warnings))]
#![feature(pin, futures_api, async_await, await_macro, arbitrary_self_types)]

use bytes::Bytes;
use futures::{
    future,
    prelude::*,
    stream::{self, StreamObj},
    task::LocalWaker,
    Poll,
};

use std::marker::Unpin;
use std::pin::Pin;

/// The raw body of an http request or response.
///
/// A body is a stream of `Bytes` values, which are shared handles to byte buffers.
/// Both `Body` and `Bytes` values can be easily created from standard owned byte buffer types
/// like `Vec<u8>` or `String`, using the `From` trait.
pub struct Body {
    stream: StreamObj<'static, Result<Bytes, std::io::Error>>,
}

impl Body {
    /// Create an empty body.
    pub fn empty() -> Self {
        Body::from_stream(stream::empty())
    }

    /// Create a body from a stream of `Bytes`
    pub fn from_stream<S>(s: S) -> Self
    where
        S: Stream<Item = Result<Bytes, std::io::Error>> + Send + 'static,
    {
        Self {
            stream: StreamObj::new(Box::new(s)),
        }
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
    fn poll_next(mut self: Pin<&mut Self>, lw: &LocalWaker) -> Poll<Option<Self::Item>> {
        Pin::new(&mut self.stream).poll_next(lw)
    }
}

pub type Request = http::Request<Body>;
pub type Response = http::Request<Body>;

pub struct HangUp;

/// An async HTTP service
///
/// An instance of a type implementing this trait represents a single open
/// connection, and may carry connection-specific state within it.
pub trait HttpService {
    type Conn;
    type ConnFut: TryFuture<Ok = Self::Conn, Error = HangUp>;
    fn connect(&self) -> Self::ConnFut;

    /// The async computation for producing the response.
    ///
    /// This API does *not* use `Result`; servers are expect to always issue
    /// a valid response to the current request.
    type Fut: TryFuture<Ok = Response, Error = HangUp>;

    /// Begin handling a single request
    fn respond(&self, conn: &mut Self::Conn, req: Request) -> Self::Fut;
}

impl<F, Fut> HttpService for F
where
    F: Fn(Request) -> Fut,
    Fut: TryFuture<Ok = Response, Error = HangUp>,
{
    type Conn = ();
    type ConnFut = future::Ready<Result<(), HangUp>>;
    fn connect(&self) -> Self::ConnFut {
        future::ok(())
    }

    type Fut = Fut;
    fn respond(&self, _: &mut (), req: Request) -> Self::Fut {
        (self)(req)
    }
}
