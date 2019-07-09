#![feature(async_await)]
use simple_logger;
use tide::middleware::DefaultHeaders;

fn main() {
    simple_logger::init_with_level(log::Level::Info).unwrap();

    let mut app = tide::App::new();

    app.middleware(
        DefaultHeaders::new()
            .header("X-Version", "1.0.0")
            .header("X-Server", "Tide"),
    );

    app.at("/").get(async move |_| "Hello, world!");

    http_service_lambda::run(app.into_http_service());
}
