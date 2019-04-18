#![feature(futures_api, async_await, await_macro, existential_type)]

use futures::{
    future::{self, FutureObj},
};
use http_service::{HttpService, Response};

struct Server {
    message: Vec<u8>,
}

impl Server {
    fn create(message: Vec<u8>) -> Server {
        Server {
            message,
        }
    }
}

impl HttpService for Server {
    type Connection = ();
    type ConnectionFuture = future::Ready<Result<(), std::io::Error>>;
    type Fut = FutureObj<'static, Result<http_service::Response, std::io::Error>>;
    
    fn connect(&self) -> Self::ConnectionFuture {
        future::ok(())
    }

    fn respond(&self, _conn: &mut (), _req: http_service::Request) -> Self::Fut {
        let message = self.message.clone();
        FutureObj::new(Box::new(
            async move {
                Ok(Response::new(http_service::Body::from(message)))
            }
        ))
    }
}