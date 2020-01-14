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
//! fn main() {
//!     let mut app = tide::new();
//!     app.at("/").get(async move |_| "Hello, world!");
//!     http_service_lambda::run(app.into_http_service());
//! }
//! ```

#![forbid(future_incompatible, rust_2018_idioms)]
#![deny(missing_debug_implementations, nonstandard_style)]
#![warn(missing_docs, missing_doc_code_examples)]
#![cfg_attr(test, deny(warnings))]

use futures::{
    channel::mpsc::{unbounded, UnboundedReceiver, UnboundedSender},
    AsyncReadExt, Future, FutureExt, StreamExt, TryFutureExt,
};
use http_service::{Body as HttpBody, HttpService, Request as HttpRequest};
use lambda_http::{lambda, Body as LambdaBody, Handler, Request as LambdaHttpRequest};
use lambda_runtime::{error::HandlerError, Context};
use std::{
    sync::mpsc::{channel as sync_channel, Sender as SyncSender},
    thread,
};
use tokio::runtime::Runtime as TokioRuntime;

type LambdaResponse = lambda_http::Response<LambdaBody>;

trait ResultExt<Ok, Error> {
    fn handler_error(self, description: &str) -> Result<Ok, HandlerError>;
}

impl<Ok, Error> ResultExt<Ok, Error> for Result<Ok, Error> {
    fn handler_error(self, description: &str) -> Result<Ok, HandlerError> {
        self.map_err(|_| HandlerError::from(description))
    }
}

type RequestSender = UnboundedSender<(LambdaHttpRequest, ResponseSender)>;
type RequestReceiver = UnboundedReceiver<(LambdaHttpRequest, ResponseSender)>;
type ResponseSender = SyncSender<Result<LambdaResponse, HandlerError>>;

struct Server<S> {
    service: S,
    requests: RequestReceiver,
}

impl<S: HttpService> Server<S> {
    fn new(service: S, requests: RequestReceiver) -> Server<S> {
        Server { service, requests }
    }

    async fn run(mut self) -> Result<(), ()> {
        while let Some((req, reply)) = self.requests.next().await {
            let response = self.serve(req).await;
            reply.send(response).unwrap();
        }
        Ok(())
    }

    async fn serve(&self, req: LambdaHttpRequest) -> Result<LambdaResponse, HandlerError> {
        // Create new connection
        let mut connection = self
            .service
            .connect()
            .into_future()
            .await
            .handler_error("connect")?;

        // Convert Lambda request to HTTP request
        let req: HttpRequest = req.map(|b| match b {
            LambdaBody::Binary(v) => HttpBody::from(v),
            LambdaBody::Text(s) => HttpBody::from(s.into_bytes()),
            LambdaBody::Empty => HttpBody::empty(),
        });

        // Handle request
        let (parts, mut body) = self
            .service
            .respond(&mut connection, req)
            .into_future()
            .await
            .handler_error("respond")?
            .into_parts();

        // Convert response back to Lambda response
        let mut buf = Vec::new();
        body.read_to_end(&mut buf).await.handler_error("body")?;
        let lambda_body = if buf.is_empty() {
            LambdaBody::Empty
        } else {
            match String::from_utf8(buf) {
                Ok(s) => LambdaBody::Text(s),
                Err(b) => LambdaBody::Binary(b.into_bytes()),
            }
        };
        Ok(LambdaResponse::from_parts(parts, lambda_body))
    }
}

struct ProxyHandler(RequestSender);

impl Handler<LambdaResponse> for ProxyHandler {
    fn run(
        &mut self,
        event: LambdaHttpRequest,
        _ctx: Context,
    ) -> Result<LambdaResponse, HandlerError> {
        let (reply, response_chan) = sync_channel();
        self.0
            .unbounded_send((event, reply))
            .handler_error("forward event")?;
        response_chan.recv().handler_error("receive response")?
    }
}

fn prepare_proxy<S: HttpService>(
    service: S,
) -> (ProxyHandler, impl Future<Output = Result<(), ()>>) {
    let (request_sender, requests) = unbounded();
    let server = Server::new(service, requests);
    (ProxyHandler(request_sender), server.run())
}

/// Serve the given `HttpService` using `lambda_http` as backend and
/// return a `Future` that can be `await`ed on.
pub fn serve<S: HttpService>(s: S) -> impl Future<Output = Result<(), ()>> {
    let (handler, server_task) = prepare_proxy(s);
    thread::spawn(|| lambda!(handler));
    server_task
}

/// Run the given `HttpService` on the default runtime, using
/// `lambda_http` as backend.
pub fn run<S: HttpService>(s: S) {
    let (handler, server) = prepare_proxy(s);
    let mut runtime = TokioRuntime::new().expect("Can not start tokio runtime");
    runtime.spawn(server.boxed().compat());
    lambda!(handler, runtime);
}

#[cfg(test)]
mod tests {
    use super::*;
    use futures::future;
    use lambda_http::Handler;

    struct DummyService;

    impl HttpService for DummyService {
        type Connection = ();
        type ConnectionFuture = future::Ready<Result<(), ()>>;
        type ResponseFuture = future::BoxFuture<'static, Result<http_service::Response, ()>>;
        fn connect(&self) -> Self::ConnectionFuture {
            future::ok(())
        }
        fn respond(&self, _conn: &mut (), _req: http_service::Request) -> Self::ResponseFuture {
            Box::pin(async move { Ok(http_service::Response::new(http_service::Body::empty())) })
        }
    }

    fn run_once(request: LambdaHttpRequest) -> Result<LambdaResponse, HandlerError> {
        let (mut handler, server) = prepare_proxy(DummyService);
        std::thread::spawn(|| futures::executor::block_on(server));
        handler.run(request, Context::default())
    }

    #[test]
    fn handle_apigw_request() {
        // from the docs
        // https://docs.aws.amazon.com/lambda/latest/dg/eventsources.html#eventsources-api-gateway-request
        let input = include_str!("../tests/data/apigw_proxy_request.json");
        let request = lambda_http::request::from_str(input).unwrap();
        let result = run_once(request);
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
        let result = run_once(request);
        assert!(
            result.is_ok(),
            format!("event was not handled as expected {:?}", result)
        );
    }
}
