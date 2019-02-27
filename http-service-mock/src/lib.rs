#![feature(futures_api, async_await)]

use futures::{executor::block_on, prelude::*};
use http_service::{HttpService, Request, Response};

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

    pub fn simulate(&mut self, req: Request) -> Result<Response, <T::Fut as TryFuture>::Error> {
        block_on(
            self.service
                .respond(&mut self.connection, req)
                .into_future(),
        )
    }
}

pub fn make_server<T: HttpService>(service: T) -> Result<TestBackend<T>, <T::ConnectionFuture as TryFuture>::Error> {
    TestBackend::wrap(service)
}
