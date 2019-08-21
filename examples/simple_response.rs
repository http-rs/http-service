use futures::future::{self, BoxFuture, FutureExt};
use http_service::{HttpService, Response};
use std::net::{IpAddr, Ipv4Addr, SocketAddr};

struct Server {
    message: Vec<u8>,
}

impl Server {
    fn create(message: Vec<u8>) -> Server {
        Server { message }
    }

    pub async fn run(s: Server) {
        let a = SocketAddr::new(IpAddr::V4(Ipv4Addr::new(127, 0, 0, 1)), 8088);
        http_service_hyper::serve(s, a).await.unwrap();
    }
}

impl HttpService for Server {
    type Connection = ();
    type ConnectionFuture = future::Ready<Result<(), std::io::Error>>;
    type ResponseFuture = BoxFuture<'static, Result<http_service::Response, std::io::Error>>;

    fn connect(&self) -> Self::ConnectionFuture {
        future::ok(())
    }

    fn respond(&self, _conn: &mut (), _req: http_service::Request) -> Self::ResponseFuture {
        let message = self.message.clone();
        async move { Ok(Response::new(http_service::Body::from(message))) }.boxed()
    }
}

#[tokio::main]
async fn main() {
    let s = Server::create(String::from("Hello, World").into_bytes());
    Server::run(s).await;
}
