use async_std::net::TcpListener;
use async_std::task;
use http_service::{Error, Response};
use http_service_h1::Server;
use http_types::StatusCode;

fn main() {
    // And an HttpService to handle each connection...
    let service = |_req| async {
        let mut res = Response::new(StatusCode::Ok);
        res.insert_header("Content-Type", "text/plain")?;
        res.set_body("Hello world".to_string());
        Ok::<Response, Error>(res)
    };

    task::block_on(async move {
        // Then bind, configure the spawner to our pool, and serve...
        let listener = TcpListener::bind("127.0.0.1:8080").await?;
        let addr = format!("http://{}", listener.local_addr()?);

        let mut server = Server::new(addr, listener.incoming(), service);
        server.run().await?;

        Ok::<(), Error>(())
    })
    .unwrap();
}
