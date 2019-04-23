//! A crate for simulating an http server

#![forbid(future_incompatible, rust_2018_idioms)]
#![deny(missing_debug_implementations, nonstandard_style)]
#![warn(missing_docs, missing_doc_code_examples)]
#![cfg_attr(test, deny(warnings))]
#![feature(futures_api, async_await)]

use futures::{executor::block_on, prelude::*};
use http_service::{HttpService, Request, Response};

/// A harness for sending simulated requests to an HTTP service
#[derive(Debug)]
pub struct TestBackend<T: HttpService> {
    service: T,
    connection: T::Connection,
}
 
impl<T: HttpService> TestBackend<T> {
    fn wrap(service: T) -> Result<Self, <T::ConnectionFuture as TryFuture>::Error> {
        let connection = block_on(service.connect().into_future())?;
        Ok(Self {
            service,
            connection,
        })
    }

    /// Send a request to the simulated server
    pub fn simulate(&mut self, req: Request) -> Result<Response, <T::Fut as TryFuture>::Error> {
        block_on(
            self.service
                .respond(&mut self.connection, req)
                .into_future(),
        )
    }
}

/// Construct a simulated http server from the given service.
pub fn make_server<T: HttpService>(
    service: T,
) -> Result<TestBackend<T>, <T::ConnectionFuture as TryFuture>::Error> {
    TestBackend::wrap(service)
}
