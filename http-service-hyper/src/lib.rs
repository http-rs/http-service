//! `HttpService` server that uses Hyper as backend.

#![forbid(future_incompatible, rust_2018_idioms)]
#![deny(missing_debug_implementations, nonstandard_style)]
#![warn(missing_docs, missing_doc_code_examples)]
#![cfg_attr(test, deny(warnings))]

use futures::prelude::*;
use http::Response;
use http_service::HttpService;
use hyper::service::{make_service_fn, service_fn};
use std::net::SocketAddr;
use std::sync::Arc;

/// Start hyper server and return a future.
pub async fn serve<CO, CE, RE, S>(s: S, addr: SocketAddr) -> Result<(), hyper::Error>
where
    CO: Send + 'static,
    CE: std::error::Error + Send + Sync + 'static,
    RE: std::error::Error + Send + Sync + 'static,
    S: HttpService<Connection = CO> + Send + 'static,
    <S as http_service::HttpService>::ConnectionFuture:
        Future<Output = Result<CO, CE>> + Send + 'static,
    <S as http_service::HttpService>::ResponseFuture:
        Future<Output = Result<Response<http_service::Body>, RE>> + Send + 'static,
{
    let s = Arc::new(s);

    let make_svc = make_service_fn({
        let s = s.clone();
        move |_| {
            let s = s.clone();
            async move {
                Ok::<_, hyper::Error>(service_fn({
                    let s = s.clone();
                    Box::new({
                        let s = s.clone();
                        move |req: http::Request<hyper::Body>| {
                            let s = s.clone();
                            Box::pin(async move {
                                let mut conn = s.connect().await?;
                                let (parts, body) = req.into_parts();
                                let body = http_service::Body::from_stream(
                                    body.map_ok(|b| b.into()).map_err(|e: hyper::Error| {
                                        std::io::Error::new(std::io::ErrorKind::Other, e)
                                    }),
                                );

                                let rsp = s
                                    .respond(&mut conn, http::Request::from_parts(parts, body))
                                    .await?;

                                let (parts, body) = rsp.into_parts();
                                let body: hyper::Body = body.into_vec().await?.into();
                                Ok::<_, Box<dyn std::error::Error + Send + Sync>>(
                                    http::Response::from_parts(parts, body),
                                )
                            })
                        }
                    })
                }))
            }
        }
    });

    hyper::Server::bind(&addr).serve(make_svc).await
}
