# Development Setup

**Requirements:** Rust 1.85+ (stable). Install via [rustup](https://rustup.rs/).

## Clone and build

```sh
git clone https://github.com/ashlar-display/hdmi-hal.git
cd hdmi-hal
cargo build
```

## Running checks

```sh
cargo fmt --check
cargo clippy -- -D warnings
cargo rustdoc -- -D missing_docs
```

## Running the simulate example

```sh
cd examples/simulate
cargo run
```