# Testing strategy

hdmi-hal defines traits and the value types they carry; it has no implementation of its
own beyond the default methods. Its tests check that the traits can be implemented and
called as documented, using small mock implementations, and that the value types keep
their invariants. Behaviour against real hardware is tested in the backends and in the
crates that drive the traits.

## Test structure

### Unit tests

Unit tests live in a `#[cfg(test)]` module at the end of each file:

| File | What the tests check |
|---|---|
| `src/phy/mod.rs` | A mock `HdmiPhy` records each call: FRL rate, scrambling, FRL output and link training patterns are applied and can be changed; the LTP values match their SCDC encoding; `TxFfeLevel` accepts 0–7 and rejects the rest; the equalization parameters and FFE restrictions keep their register order and default to no restriction; the value types can be hashed |
| `src/scdc/mod.rs` | `read_block_bytewise`, the fallback for transports that cannot burst-read: it reads consecutive registers, reads nothing for an empty buffer, stops at the first error, wraps past `0xFF`, and works through a trait object; a mock transport's writes read back |

### Doctests

The README's Rust examples run as doctests (`ReadmeDoctests` in `src/lib.rs`). The
backend example is a sketch: its error type is defined in a hidden line and its method
bodies are `todo!()`, so it is compiled against the trait but never run.

### The simulate example

`examples/simulate` is a separate crate with its own lockfile: a simulated SCDC
transport and PHY driven through the traits. CI runs it, so it keeps compiling and
running against the current traits.

## Builds

The crate has no features. CI runs the tests on stable and on the MSRV, and builds the
crate for `thumbv7em-none-eabi`, which has no `std`, on both.

## Coverage

Coverage is measured with `cargo-llvm-cov` and kept from falling by a ratchet: CI fails
when line coverage drops below `.coverage-baseline` (with a 0.1 point tolerance), and
opens a PR raising the baseline when a push improves it. See
[setup.md](setup.md#measuring-coverage) for the commands.

## Philosophy

- **Test the contract, not an implementation.** A mock records what the trait was asked
  to do; the tests check the calls and values the documentation promises.
- **Invariants at construction.** Value types that restrict their range (`TxFfeLevel`)
  are tested at both ends of it.
