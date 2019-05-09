<h1 align="center">http-service</h1>
<div align="center">
 <strong>
    Types and traits to help you implement your own HTTP server
 </strong>
</div>

<br />

<div align="center">
  <!-- Crates version -->
  <a href="https://crates.io/crates/http-service">
    <img src="https://img.shields.io/crates/v/http-service.svg?style=flat-square"
    alt="Crates.io version" />
  </a>
  <!-- Build Status -->
  <a href="https://travis-ci.org/rustasync/http-service">
    <img src="https://img.shields.io/travis/rustasync/http-service.svg?style=flat-square"
      alt="Build Status" />
  </a>
  <!-- Downloads -->
  <a href="https://crates.io/crates/http-service">
    <img src="https://img.shields.io/crates/d/http-service.svg?style=flat-square"
      alt="Download" />
  </a>
  <!-- docs.rs docs -->
  <a href="https://docs.rs/http-service/0.1.5/http_service">
    <img src="https://img.shields.io/badge/docs-latest-blue.svg?style=flat-square"
      alt="docs.rs docs" />
  </a>
</div>

<div align="center">
  <h3>
    <a href="https://docs.rs/http-service/0.1.5/http_service/">
      API Docs
    </a>
    <span> | </span>
    <a href="https://discordapp.com/channels/442252698964721669/474974025454452766">
      Chat
    </a>
  </h3>
</div>

<div align="center">
  <sub>Built with ⛵ by <a href="https://github.com/rustasync">The Rust Async Ecosystem WG</a>
</div>

## About
The crate `http-service` provides the necessary types and traits to implement your own HTTP Server. It uses `hyper` for the lower level TCP abstraction. 

You can use the workspace member [`http-service-hyper`](https://crates.io/crates/http-service-hyper) to run your HTTP Server.

1. Runs via `http_service_hyper::run(HTTP_SERVICE, ADDRESS);`
2. Returns a future which can be `await`ed via `http_service_hyper::serve(HTTP_SERVICE, ADDRESS);`

This crate uses the latest [Futures](https://github.com/rust-lang-nursery/futures-rs) preview, and therefore needs to be run on Rust Nightly.

## Examples

**Cargo.toml**
```
[dependencies]
http-service = "0.1.5"
futures-preview = "0.3.0-alpha.14"

[dependencies.http-service-hyper]
version = "0.1.1"
```

**main.rs**
```rust
#![feature(futures_api, async_await, await_macro, existential_type)]

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
```

## License

[MIT](./LICENSE-MIT) OR [Apache-2.0](./LICENSE-APACHE)
