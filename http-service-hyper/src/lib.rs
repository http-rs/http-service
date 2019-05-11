//! `HttpService` server that uses Hyper as backend.

#![forbid(future_incompatible, rust_2018_idioms)]
#![deny(missing_debug_implementations, nonstandard_style)]
#![warn(missing_docs, missing_doc_code_examples)]
#![cfg_attr(test, deny(warnings))]
#![feature(async_await)]

use futures::{
    compat::{Compat, Compat01As03, Future01CompatExt},
    future::BoxFuture,
    prelude::*,
};
use http_service::{Body, HttpService};
use std::{net::SocketAddr, sync::Arc};

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
    type Future = Compat<BoxFuture<'static, Result<Self::Service, Self::Error>>>;
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
            .compat()
    }
}

impl<H> hyper::service::Service for WrapConnection<H>
where
    H: HttpService,
{
    type ReqBody = hyper::Body;
    type ResBody = hyper::Body;
    type Error = std::io::Error;
    type Future = Compat<BoxFuture<'static, Result<http::Response<hyper::Body>, Self::Error>>>;

    fn call(&mut self, req: http::Request<hyper::Body>) -> Self::Future {
        let error = std::io::Error::from(std::io::ErrorKind::Other);
        let req = req.map(|hyper_body| {
            let stream = Compat01As03::new(hyper_body).map(|c| match c {
                Ok(chunk) => Ok(chunk.into_bytes()),
                Err(e) => Err(std::io::Error::new(std::io::ErrorKind::Other, e)),
            });
            Body::from_stream(stream)
        });
        let fut = self.service.respond(&mut self.connection, req);

        async move {
            let res: http::Response<_> = fut.into_future().await.map_err(|_| error)?;
            Ok(res.map(|body| hyper::Body::wrap_stream(body.compat())))
        }
            .boxed()
            .compat()
    }
}

/// Serve the given `HttpService` at the given address, using `hyper` as backend, and return a
/// `Future` that can be `await`ed on.
pub fn serve<S: HttpService>(
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
pub fn run<S: HttpService>(s: S, addr: SocketAddr) {
    let server = serve(s, addr).map(|_| Result::<_, ()>::Ok(())).compat();
    hyper::rt::run(server);
}
