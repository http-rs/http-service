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

## About

The crate `http-service` provides the necessary types and traits to implement
your own HTTP Server.

For example you can use the workspace member
[`http-service-hyper`](https://crates.io/crates/http-service-hyper) to run an
HTTP Server.

1. Runs via `http_service_hyper::run(HTTP_SERVICE, ADDRESS);`
2. Returns a future which can be `await`ed via
   `http_service_hyper::serve(HTTP_SERVICE, ADDRESS);`

## Contributing

Want to join us? Check out our [The "Contributing" section of the
guide][contributing] and take a look at some of these issues:

- [Issues labeled "good first issue"][good-first-issue]
- [Issues labeled "help wanted"][help-wanted]

#### Conduct

The http-service project adheres to the [Contributor Covenant Code of
Conduct](https://github.com/http-rs/.github/blob/master/CODE_OF_CONDUCT.md).
This describes the minimum behavior expected from all contributors.

## License

[MIT](./LICENSE-MIT) OR [Apache-2.0](./LICENSE-APACHE)

[contributing]: https://github.com/http-rs/.github/blob/master/CONTRIBUTING.md
[good-first-issue]: https://github.com/http-rs/http-service/labels/good%20first%20issue
[help-wanted]: https://github.com/http-rs/http-service/labels/help%20wanted
