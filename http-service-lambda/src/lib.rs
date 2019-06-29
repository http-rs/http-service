//! `HttpService` server that uses AWS Lambda Rust Runtime as backend.
//!
//! This crate builds on the standard http interface provided by the
//! [lambda_http](https://docs.rs/lambda_http) crate and provides a http server
//! that runs on the lambda runtime.
//!
//! Compatible services like [tide](https://github.com/rustasync/tide) apps can
//! run on lambda and processing events from API Gateway or ALB without much
//! change.
//!
//! # Examples
//!
//! **Hello World**
//!
//! ```rust,ignore
//! #![feature(async_await)]
//!
//! fn main() {
//!     let mut app = tide::App::new();
//!     app.at("/").get(async move |_| "Hello, world!");
//!     http_service_lambda::run(app.into_http_service());
//! }
//! ```

#![forbid(future_incompatible, rust_2018_idioms)]
#![deny(missing_debug_implementations, nonstandard_style)]
#![warn(missing_docs, missing_doc_code_examples)]
#![cfg_attr(test, deny(warnings))]
#![feature(async_await)]

use futures::{FutureExt, TryFutureExt};
use http_service::{Body as HttpBody, HttpService, Request as HttpRequest};
use lambda_http::{Body as LambdaBody, Handler, Request as LambdaHttpRequest};
use lambda_runtime::{error::HandlerError, Context};
use std::future::Future;
use std::sync::Arc;
use tokio::runtime::Runtime as TokioRuntime;

type LambdaResponse = lambda_http::Response<lambda_http::Body>;

trait ResultExt<OK, ERR> {
    fn handler_error(self, description: &str) -> Result<OK, HandlerError>;
}

impl<OK, ERR> ResultExt<OK, ERR> for Result<OK, ERR> {
    fn handler_error(self, description: &str) -> Result<OK, HandlerError> {
        self.map_err(|_| HandlerError::from(description))
    }
}

trait CompatHttpBodayAsLambda {
    fn into_lambda(self) -> LambdaBody;
}

impl CompatHttpBodayAsLambda for Vec<u8> {
    fn into_lambda(self) -> LambdaBody {
        if self.is_empty() {
            return LambdaBody::Empty;
        }
        match String::from_utf8(self) {
            Ok(s) => LambdaBody::from(s),
            Err(e) => LambdaBody::from(e.into_bytes()),
        }
    }
}

struct Server<S> {
    service: Arc<S>,
    rt: TokioRuntime,
}

impl<S> Server<S>
where
    S: HttpService,
{
    fn new(s: S) -> Server<S> {
        Server {
            service: Arc::new(s),
            rt: tokio::runtime::Runtime::new().expect("failed to start new Runtime"),
        }
    }

    fn serve(
        &self,
        req: LambdaHttpRequest,
    ) -> impl Future<Output = Result<LambdaResponse, HandlerError>> {
        let service = self.service.clone();
        async move {
            let req: HttpRequest = req.map(|b| HttpBody::from(b.as_ref()));
            let mut connection = service
                .connect()
                .into_future()
                .await
                .handler_error("connect")?;
            let (parts, body) = service
                .respond(&mut connection, req)
                .into_future()
                .await
                .handler_error("respond")?
                .into_parts();
            let resp = LambdaResponse::from_parts(
                parts,
                body.into_vec().await.handler_error("body")?.into_lambda(),
            );
            Ok(resp)
        }
    }
}

impl<S> Handler<LambdaResponse> for Server<S>
where
    S: HttpService,
{
    fn run(
        &mut self,
        req: LambdaHttpRequest,
        _ctx: Context,
    ) -> Result<LambdaResponse, HandlerError> {
        // Lambda processes one event at a time in a Function. Each invocation
        // is not in async context so it's ok to block here.
        self.rt.block_on(self.serve(req).boxed().compat())
    }
}

/// Run the given `HttpService` on the default runtime, using `lambda_http` as
/// backend.
pub fn run<S: HttpService>(s: S) {
    let server = Server::new(s);
    // Let Lambda runtime start its own tokio runtime
    lambda_http::start(server, None);
}

#[cfg(test)]
mod tests {
    use super::*;
    use futures::future;

    struct DummyService;

    impl HttpService for DummyService {
        type Connection = ();
        type ConnectionFuture = future::Ready<Result<(), ()>>;
        type Fut = future::BoxFuture<'static, Result<http_service::Response, ()>>;
        fn connect(&self) -> Self::ConnectionFuture {
            future::ok(())
        }
        fn respond(&self, _conn: &mut (), _req: http_service::Request) -> Self::Fut {
            Box::pin(async move { Ok(http_service::Response::new(http_service::Body::empty())) })
        }
    }

    #[test]
    fn handle_apigw_request() {
        // from the docs
        // https://docs.aws.amazon.com/lambda/latest/dg/eventsources.html#eventsources-api-gateway-request
        let input = include_str!("../tests/data/apigw_proxy_request.json");
        let request = lambda_http::request::from_str(input).unwrap();
        let mut handler = Server::new(DummyService);
        let result = handler.run(request, Context::default());
        assert!(
            result.is_ok(),
            format!("event was not handled as expected {:?}", result)
        );
    }

    #[test]
    fn handle_alb_request() {
        // from the docs
        // https://docs.aws.amazon.com/elasticloadbalancing/latest/application/lambda-functions.html#multi-value-headers
        let input = include_str!("../tests/data/alb_request.json");
        let request = lambda_http::request::from_str(input).unwrap();
        let mut handler = Server::new(DummyService);
        let result = handler.run(request, Context::default());
        assert!(
            result.is_ok(),
            format!("event was not handled as expected {:?}", result)
        );
    }
}
