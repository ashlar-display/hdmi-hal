# Architecture

## Role

`hdmi-hal` defines the behavioral contracts between protocol logic and hardware across
the HDMI stack. It is a traits-only crate: no implementations live here. Every trait
in this crate expresses an I/O boundary that multiple crates in the stack need to cross
in a compatible way.

The bar for inclusion is that at least two crates must share the contract. Traits that
are only ever used by a single library stay in that library.

---

## Scope

`hdmi-hal` covers:

- `ScdcTransport` — raw register read/write access to the SCDC register map over DDC/I²C
- `HdmiPhy` — transmitter PHY control: FRL rate, link training patterns, FRL output
  mode, scrambling and equalization
- CEC line access trait — when CEC is implemented

The following are explicitly out of scope:

- **Protocol logic** — SCDC register semantics, typed register wrappers, and the FRL
  link training state machine belong in the crates that consume `ScdcTransport`.
- **Implementations** — concrete platform backends (kernel I²C drivers, simulator
  harnesses, test doubles) live in the crates that use these traits. No implementation
  of any trait in this crate lives in this crate.
- **PHY vendor backends** — hardware-specific register sequences belong in platform
  crates that implement `HdmiPhy`.
- **CEC or eARC protocol logic** — only the wire-access primitive belongs here; the
  protocol state machines are in their own crates.

---

## Dependency Position

`hdmi-hal` depends on `display-types` for shared HDMI vocabulary types (`HdmiForumFrl`,
`FrlRate`). It does not depend on `piaf`, `concordance`, or any other stack crate.

```
display-types   ←   hdmi-hal        ←   culvert
                                    ←   plumbob
                                    ←   (CEC)
                                    ←   platform backends (sync)

display-types   ←   hdmi-hal-async  ←   culvert-async
hdmi-hal        ↗                   ←   plumbob-async
                                    ←   platform backends (Embassy / async)
```

`display-types` does not depend on `hdmi-hal`. The two crates share no circular
dependency; `display-types` defines vocabulary, `hdmi-hal` defines contracts.

`hdmi-hal-async` is a companion crate that mirrors each trait in `hdmi-hal` with
`async fn` methods and identical signatures otherwise. It depends on `hdmi-hal` to
reuse shared data types (`EqParams` and any future structs) rather than duplicating
them. Implementors target one or both crates depending on their executor environment;
consumers of both sync and async backends accept the relevant trait bound.

---

## Traits

### `ScdcTransport`

Abstracts raw register access to the SCDC register map over DDC/I²C.

SCDC is a bidirectional channel used for FRL link training initiation, scrambling
control, and CED (Character Error Detection) reporting. Any code that needs to touch
the SCDC register map does so through this trait.

```rust
pub trait ScdcTransport {
    type Error;

    fn read(&self, reg: u8) -> Result<u8, Self::Error>;
    fn write(&mut self, reg: u8, value: u8) -> Result<(), Self::Error>;

    /// One DDC transaction for the whole buffer, or an error. Required.
    fn read_block(&self, reg: u8, buf: &mut [u8]) -> Result<(), Self::Error>;
}

/// One `read` per byte: for a transport that cannot burst-read.
pub fn read_block_bytewise<T: ScdcTransport + ?Sized>(
    transport: &T,
    reg: u8,
    buf: &mut [u8],
) -> Result<(), T::Error> { ... }
```

The trait operates at the raw register level: a one-byte address and a one-byte value.
The typed SCDC register wrappers — named constants, bitfield structs, multi-register
sequences — belong in the `culvert` crate, not here. Keeping this boundary clean means
`hdmi-hal` encodes no knowledge of the SCDC specification; it only describes how bytes
move.

The associated `Error` type is bounded by the implementing crate. A hardware backend
exposes its own I²C error type; a simulator may use `Infallible`.

**Block reads.** `read_block` reads consecutive registers into a buffer in one
transaction, so that the transport does not split related registers, such as the status
flags the link training crate polls or the CED counters. The full contract is in the
trait's documentation, the single source of truth; in short:

- `buf[i]` is register `reg + i`, read as one combined transfer to the layer below (on
  DDC: offset write, repeated START, `buf.len()` bytes read, STOP) or not at all: an
  implementation never splits the read and never falls back to one register at a time,
  and concurrent calls do not interleave on the bus. Over an OS bus driver the transfer
  is handed over whole; a driver known to split it on the wire (the Synopsys DesignWare
  HDMI controller's I²C master reads byte by byte, `dw_hdmi_i2c_read` in Linux) is named
  in the transport's documentation.
- One transaction is not a snapshot: whether the sink holds its registers still is up to
  the sink. The CED checksum (culvert) is the one protection that does not depend on
  either.
- An implementation may have a documented maximum length of at least 16 bytes; a longer
  buffer is an error, not split. 16 is generic, not SCDC-shaped: it covers the CED block
  (9 bytes) with room, and a register dump reads in 16-byte parts on any transport.
- An empty buffer is `Ok` with no bus activity; on error the buffer is unspecified; a
  read past `0xFF` is unspecified, but never panics.
- After an error the transport should stay usable (the trait's "Errors"); one that cannot
  documents how to recover.

Not in the contract yet, and additive when a consumer can use them: a way for a transport
to say whether its block reads are atomic, and its maximum length, as methods with
defaults (unknown; the 16-byte minimum).

The method is required. A transport that cannot burst-read implements it with
`read_block_bytewise`, one `read` per byte, the documented exception to the "one
transaction" rule. An earlier version had that loop as a default implementation; it was
removed because a transport whose author never thought about block reads would then
tear related registers without anyone noticing. With a required method, the choice is
visible in every transport's code. The helper is generic over the trait, not an
implementation for any platform, so it is consistent with "contracts only" below.

There is no block write: no consumer needs one yet.

---

### `HdmiPhy`

Covers what FRL link training needs from an HDMI 2.1 transmitter PHY: FRL rate
selection, link training patterns, FRL output mode, scrambling, and each lane's TxFFE
level (FRL's transmitter equalization, a combination of pre-shoot and de-emphasis). Further
PHY configuration is not covered yet; see "Not covered yet" below.

```rust
pub trait HdmiPhy {
    type Error;

    /// Select the FRL rate (or TMDS). Triggers the required lane reconfiguration sequence.
    fn set_frl_rate(&mut self, rate: HdmiForumFrl) -> Result<(), Self::Error>;

    /// Drive the given link training patterns on the physical lanes, one per lane.
    fn send_ltp(&mut self, patterns: LanePatterns) -> Result<(), Self::Error>;

    /// Select what the transmitter sends on the FRL lanes: gap characters only, or video,
    /// data islands and control periods.
    fn set_frl_output(&mut self, output: FrlOutput) -> Result<(), Self::Error>;

    /// Adjust equalization parameters after link training feedback.
    fn adjust_equalization(&mut self, params: EqParams) -> Result<(), Self::Error>;

    /// Enable or disable scrambling on the PHY.
    fn set_scrambling(&mut self, enabled: bool) -> Result<(), Self::Error>;
}
```

`HdmiForumFrl` is from `display-types`. `LtpPattern` is a non-exhaustive enum defined in
this crate naming the patterns a PHY can drive, with the SCDC values as discriminants
(1 all ones, 2 all zeros, 3 Nyquist clock, 4 DDE compliance, 5–8 LFSR 0–3). The PHY
has to know what each pattern is in order to generate it, so the meaning of each value
is part of the contract between the link training crate and the PHY, and is named here
rather than left to each backend. The link training crate keeps its own `LtpReq` type
for the sink's raw requests, which include values that are not patterns (no pattern,
TxFFE and rate change requests), and maps the pattern requests to `LtpPattern` before
calling the PHY. Defining `LtpPattern` here keeps `hdmi-hal` free of any dependency on
`plumbob`. `send_ltp` takes `LanePatterns`, the full per-lane set: `lane0` to `lane3`,
each an `Option<LtpPattern>` where `None` means no training pattern on that lane (and
`lane3` is `None` in 3-lane FRL mode). The PHY applies the set as given; the link training
crate tracks which pattern each lane carries, since a sink's request can leave a lane's
previous pattern in place. `EqParams` carries the equalization the link training crate
sets as the sink requests higher TxFFE levels (an FFE change, 0xE): `lane0`, `lane1`,
`lane2` (`LaneEqParams`) and `lane3` (`Option<LaneEqParams>`, `None` in 3-lane FRL mode). `LaneEqParams` carries the lane's
TxFFE level as a `TxFfeLevel`, a newtype that only holds 0–7, so a PHY backend never sees
an out-of-range level. The lower maximum that applies at some FRL rates is advertised to
the sink by the link training crate, which keeps each lane's level within it.
`LaneEqParams` stays `#[non_exhaustive]` so further per-lane settings can be added.

`EqParams` also carries `tx_ffe_restrictions`: the three TxFFE bits a compliance tester
can set in `Source_Test_Configuration` (pre-shoot only, de-emphasis only, no FFE), for the
whole link, exactly as written; no bit set means unrestricted TxFFE for every backend.
Combining them is left to the PHY, which documents its choice: the reference
implementations disagree (the AMD display driver treats both "only" bits as leaving
neither component and lets no FFE override, the Rockchip HDPTX PHY driver treats them as
full FFE and ignores no FFE), and the specification text is not available. A PHY that
cannot restrict its TxFFE may ignore them and says so. The Rockchip driver applies them
only after the sink requests the DDE compliance pattern (LTP 4) on a lane, the AMD driver
with every TxFFE setting. Every `adjust_equalization` call carries the complete settings,
restrictions included.

`set_frl_output` takes `FrlOutput`: `GapOnly` (gap characters only, during link training
and while waiting for the sink to start FRL) or `Active` (video, data islands and control
periods, set once training has succeeded). It is part of `HdmiPhy` because both the link
training crate and the integration layer drive it, and because `HdmiPhy` already carries
link-level operations such as `set_scrambling` and `send_ltp`.

`FrlOutput` is deliberately not `#[non_exhaustive]`, unlike `LtpPattern`. The patterns
are a list that later HDMI versions can extend, and a PHY that cannot generate a new one
can report an error that ends the training attempt. The two output modes are the two
states of an FRL link, training and carrying content. A PHY has to implement both, and
has no sensible behaviour for a mode it does not know: a wildcard arm could only fail.
If a later HDMI version adds a mode, it should reach every implementation as a compile
error in a breaking release, not as a runtime error from a PHY that was never updated.

Like `ScdcTransport`, implementations are entirely in platform crates. The trait surface
is driven by what the link training and mode-setting layers need to call; vendor-specific
register sequences are an implementation detail.

**Not covered yet.** Today's surface is what link training needs. The mode-setting layer
and platform bring-up will also need lane mapping (lane order and polarity), TMDS
pre-emphasis and swing, spread-spectrum clocking and power states. Following stack-ordered
delivery (see "Design Principles"), each is decided when the layer that needs it is built,
including whether it belongs in the trait or in a backend's own configuration: lane
mapping, for example, is fixed by the board and may be set when the backend is
constructed. New trait methods with default implementations are not breaking changes;
required ones would be. `doc/roadmap.md` tracks them.

---

### CEC Line Trait _(planned)_

Will abstract single-wire CEC bus access: bit-bang read, write, and collision detection.
Protocol state machines (device discovery, logical address allocation, command dispatch)
are not here; only the wire primitive. Will be defined when CEC implementation begins.

---

## `no_std` Compatibility

All traits in this crate must be usable in bare `no_std` environments. This means:

- No trait method may require allocation.
- `Error` associated types may be `Infallible` in no-alloc implementations.
- No default implementations may bring in `std` dependencies.
- Dependencies are declared with `default-features = false`, so they cannot enable `std`
  for crates further up the stack. CI builds the crate for `thumbv7em-none-eabi`, a
  target without `std`.

---

## Async Compatibility

`hdmi-hal` defines synchronous traits. Async variants — identical in shape, with
`async fn` methods — live in the companion crate `hdmi-hal-async`, following the same
split as `embedded-hal` / `embedded-hal-async`.

The rules that make this work cleanly:

- **Trait shapes are identical.** `hdmi-hal-async::ScdcTransport` and
  `hdmi-hal-async::HdmiPhy` mirror their sync counterparts exactly; only method
  signatures gain `async`. No behavioral differences, no extra associated types.
- **Data types are shared.** `EqParams` and any other plain data structs are defined
  in `hdmi-hal` and re-exported or referenced from `hdmi-hal-async`. There is one
  definition of each type regardless of which trait variant the caller uses.
- **Implementors choose one or both.** A bare-metal driver implements `hdmi-hal`
  traits. An Embassy driver implements `hdmi-hal-async` traits. A simulator can
  implement both. No crate is required to support both.
- **Consumer crates are generic over the trait bound.** A link training crate that
  needs async I/O takes `T: hdmi_hal_async::ScdcTransport`; one that is sync takes
  `T: hdmi_hal::ScdcTransport`. The HAL layer imposes no executor dependency on sync
  consumers.

The async companion is the separate crate `hdmi-hal-async`. It mirrors these traits with
`async fn` methods and reuses this crate's data types, so adding it required no changes
here.

---

## Simulated Implementations

`hdmi-hal` defines the contracts; it does not provide test doubles or simulators.
Each consuming crate is responsible for its own simulated transport. This is the same
pattern as piaf's handler abstraction: the library defines the interface and ships
tests against real (byte-slice) data; the integrator provides the platform backend.

A typical test harness for SCDC work:

```rust
struct SimulatedScdc {
    registers: [u8; 256],
}

impl ScdcTransport for SimulatedScdc {
    type Error = Infallible;

    fn read(&self, reg: u8) -> Result<u8, Infallible> {
        Ok(self.registers[reg as usize])
    }

    fn write(&mut self, reg: u8, value: u8) -> Result<(), Infallible> {
        self.registers[reg as usize] = value;
        Ok(())
    }

    fn read_block(&self, reg: u8, buf: &mut [u8]) -> Result<(), Infallible> {
        read_block_bytewise(self, reg, buf)
    }
}
```

The link training state machine can be driven to any desired state by pre-loading the
simulated register array, without requiring real hardware or a kernel I²C driver.

---

## Design Principles

- **Contracts only.** No implementation of any trait lives in this crate. The moment an
  implementation appears here, it becomes a platform assumption baked into a foundational
  dependency.
- **Spec accuracy and completeness.** Trait surfaces must not omit operations the
  specification defines. A method left out because it seems unlikely to be needed now
  is a missing contract that forces a breaking change or a fork later. What callers
  choose to use is their decision; what the spec defines is the scope.
- **Stack-ordered delivery.** The 0.1.0 release scope covers what the next layer in the
  stack needs to function. Everything else the spec defines still gets built, but is
  tracked on the roadmap until the layer that needs it is being implemented. This is a
  sequencing decision, not a judgement about what is worth implementing.
- **Raw boundaries, typed protocols.** `ScdcTransport` is intentionally byte-level.
  The typed SCDC register map belongs in the protocol crate. Mixing them would encode
  HDMI specification knowledge into what should be a pure I/O contract.
- **Deterministic and testable.** Every consumer of a trait in this crate must be fully
  exercisable with a simulated implementation. Hardware is never a test dependency.
- **No unsafe code.** `#![forbid(unsafe_code)]` is a hard constraint. Hardware register
  access via unsafe is the implementing crate's concern, not this crate's.
- **Extensible without forking.** The trait is the extension point. A platform with a
  proprietary PHY, a non-standard I²C stack, or a simulated transport for testing
  implements the relevant trait and plugs in — no source modification required anywhere
  in the stack. This is the HAL layer's primary contribution to that stack-wide property.
- **Stable contracts.** Trait changes are breaking changes for every implementor. The bar
  for modifying a trait surface after initial publication is high; additions go through
  careful review of what all known consumers require.
- **Sync / async parity.** The sync traits in this crate and the async traits in
  `hdmi-hal-async` must stay in lockstep. A change to a trait surface here requires a
  corresponding change to its async mirror. This is enforced by convention, not by the
  type system.
- **Attested releases.** Every release is published through a GitHub Actions workflow
  that signs the `.crate` package with [SLSA Build Level 2](https://slsa.dev) provenance.
  Verify with `gh attestation verify <file> --repo DracoWhitefire/hdmi-hal`.
