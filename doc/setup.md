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
cargo clippy --all-targets -- -D warnings
cargo rustdoc --all-features -- -D warnings
```

## Running tests

```sh
cargo test
```

The README's examples run as doctests with the others. See [testing.md](testing.md) for
what the tests cover.

## `no_std` builds

The crate is `no_std` without `alloc`. CI checks it on a bare target, which has no `std`
for a dependency to pull in:

```sh
rustup target add thumbv7em-none-eabi
cargo build --target thumbv7em-none-eabi
```

## Checking the MSRV

The MSRV is `rust-version` in `Cargo.toml`. CI checks it with
[`cargo-msrv`](https://github.com/foresterre/cargo-msrv), which runs the checks on that
toolchain:

```sh
cargo install cargo-msrv --version 0.19.6 --locked
cargo msrv verify -- cargo test --locked
cargo msrv verify --target thumbv7em-none-eabi \
  -- cargo build --locked --no-default-features --target thumbv7em-none-eabi
```

## Measuring coverage

Install [`cargo-llvm-cov`](https://github.com/taiki-e/cargo-llvm-cov) and the LLVM tools
component, then measure as CI does:

```sh
rustup component add llvm-tools-preview
cargo install cargo-llvm-cov --locked
cargo llvm-cov clean --workspace
cargo llvm-cov --no-report --locked
cargo llvm-cov report --summary-only
```

CI fails when line coverage drops more than 0.1 points below `.coverage-baseline`, and
opens a PR raising the baseline when a push improves it.

## Running the simulate example

```sh
cargo run --locked --manifest-path examples/simulate/Cargo.toml
```

The example is its own workspace with a tracked `Cargo.lock`, which records hdmi-hal's
version and dependencies through the path dependency. After changing `Cargo.toml`,
refresh it and commit it, or the `--locked` run (here and in CI) fails:

```sh
cargo check --manifest-path examples/simulate/Cargo.toml
```
