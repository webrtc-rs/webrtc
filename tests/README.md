
# End To End Tests

This is some info and suggested guidelines about the e2e test suite in its nacent stage. Updates to this document are welcome and encouraged.

## General Info

These tests are based on the tests in the `e2e/` directory of the [pion repo](https://github.com/pion/dtls), commit hash `0ffdc881a193f50218eaa21e6de784b4de93fdc6`. They have been significantly refactored to meet rust's memory safety requirement without increasing the complexity of lifetime management, but the general logic is the same: create configurations for a client and server, run the client and server in an async runtime, and determine if they were able to communicate in the expected way.

## Architecture and Use

The tests in this directory can be run with `cargo test` from the root of the project (the directory with `Cargo.toml`).

* The primary test harnesses are:
    * test_mods::e2e_simple
    * test_mods::e2e_simple_psk
    * test_mods::e2e_mtu
* Run a harness by creating one `ConfigBuilder` for client and one `ConfigBuilder` for the server and passing both to a harness. The harness will modify the builder, instantiate the `Config`, and run the tests.

## Things to Note

* Only the main thread should panic
* Errors are passed using `Result<_, String>` to the main thread

## TODOs

* Coverage report for e2e test components
* Implement unit tests for test components in `util_tests.rs`
* Factor `test_mods.rs` into several smaller files
* Bring `lossy.rs` into conformance with `basic.rs` and `openssl.rs`
