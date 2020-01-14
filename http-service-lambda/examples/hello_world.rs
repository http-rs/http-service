use http_service_lambda;
use simple_logger;

fn main() {
    simple_logger::init_with_level(log::Level::Info).unwrap();

    let mut app = tide::new();
    app.at("/").get(|_| async move { "Hello, world!" });

    http_service_lambda::run(app.into_http_service());
}
