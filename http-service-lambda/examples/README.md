## Quick Start

See the document for
[aws-lambda-rust-runtime](https://github.com/awslabs/aws-lambda-rust-runtime)
for how to build and deploy a lambda function.

Alternatively, build the example and deploy to lambda manually:

1. Build with musl target

    ```sh
    rustup target add x86_64-unknown-linux-musl
    cargo build --release --example hello_world --target x86_64-unknown-linux-musl
    ```

2. Package

    ```sh
    cp ../../target/x86_64-unknown-linux-musl/release/examples/hello_world bootstrap
    zip lambda.zip bootstrap
    ```

3. Use [AWS CLI](https://aws.amazon.com/cli/) or AWS console to create new lambda function.
