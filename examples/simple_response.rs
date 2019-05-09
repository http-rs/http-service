#![feature(async_await, await_macro, existential_type)]

use futures::future::{self, FutureObj};
use http_service::{HttpService, Response};
use std::net::{IpAddr, Ipv4Addr, SocketAddr};

struct Server {
    message: Vec<u8>,
}

impl Server {
    fn create(message: Vec<u8>) -> Server {
        Server { message }
    }

    pub fn run(s: Server) {
        let a = SocketAddr::new(IpAddr::V4(Ipv4Addr::new(127, 0, 0, 1)), 8080);
        http_service_hyper::run(s, a);
    }
}

impl HttpService for Server {
    type Connection = ();
    type ConnectionFuture = future::Ready<Result<(), std::io::Error>>;
    type ResponseFuture = FutureObj<'static, Result<http_service::Response, std::io::Error>>;

    fn connect(&self) -> Self::ConnectionFuture {
        future::ok(())
    }

    fn respond(&self, _conn: &mut (), _req: http_service::Request) -> Self::ResponseFuture {
        let message = self.message.clone();
        FutureObj::new(Box::new(async move {
            Ok(Response::new(http_service::Body::from(message)))
        }))
    }
}

fn main() {
    let s = Server::create(String::from("Hello, World").into_bytes());
    Server::run(s);
}
