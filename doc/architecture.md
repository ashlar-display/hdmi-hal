# Architecture

## Role

`hdmi-hal` defines the behavioral contracts between protocol logic and hardware across
the HDMI stack. It holds traits, and the glue between its own traits
(`read_block_bytewise`, `ScdcOverDdc`): no hardware, platform or clock implementations, no
policy. Every trait in this crate expresses an I/O boundary that multiple crates in the
stack need to cross in a compatible way.

The bar for inclusion is that at least two crates must share the contract. Traits that
are only ever used by a single library stay in that library.

---

## Scope

`hdmi-hal` covers:

- `ScdcTransport` — raw register read/write access to the SCDC register map over DDC/I²C
- `DdcBus` — the connector's whole DDC bus: any 7-bit target, and E-DDC reads of EDID and
  DisplayID; with `ScdcOverDdc`, an `ScdcTransport` over it
- `FrlPhy` — what FRL link training needs from the transmitter PHY: FRL rate, link
  training patterns, FRL output mode and equalization
- `TmdsPhy` — what TMDS bring-up and tear-down need from the transmitter PHY: character
  rate and clock ratio, the source's scrambler, starting and stopping the output
- `PhyPower` — powering the PHY off and on, shared by both mode traits
- `Clock` and `Delay` — monotonic time and waiting, for the procedures that drive these
  traits, with the `Timestamp` the clock reads
- CEC line access trait — when CEC is implemented

The following are explicitly out of scope:

- **Protocol logic** — SCDC register semantics, typed register wrappers, and the FRL
  link training state machine belong in the crates that consume `ScdcTransport`.
- **Implementations** — concrete platform backends (kernel I²C drivers, simulator
  harnesses, test doubles) live in the crates that use these traits. The only
  implementations here are glue between this crate's own traits: `ScdcOverDdc`, an
  `ScdcTransport` over any `DdcBus`, and the `read_block_bytewise` helper.
- **PHY vendor backends** — hardware-specific register sequences belong in platform
  crates that implement `FrlPhy` and `TmdsPhy`.
- **Clocks and timers** — `Clock` and `Delay` are contracts; the platform's counter and
  timer implement them, outside this crate.
- **CEC or eARC protocol logic** — only the wire-access primitive belongs here; the
  protocol state machines are in their own crates.

---

## Dependency Position

`hdmi-hal` depends on `display-types` for shared HDMI vocabulary types (`HdmiForumFrl`,
`FrlRate`, and `ColorBitDepth` and `ColorFormat` in `TmdsVideo`). It does not depend on
`piaf`, `concordance`, or any other stack crate.

```
display-types   ←   hdmi-hal        ←   culvert
                                    ←   plumbob
                                    ←   spiritlevel (its procedures, planned)
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
reuse shared data types (`EqParams`, `TmdsPhyConfig`, `Timestamp` and the others)
rather than duplicating them. Implementors target one or both crates depending on their executor environment;
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
`hdmi-hal` encodes no knowledge of the SCDC register map; it only describes how bytes
move.

The associated `Error` type is the implementing crate's, bounded by `DdcError` (see
`DdcBus` below): every transport error carries a `DdcErrorKind`, so transport-generic code
can tell a sink that does not answer from a bus that timed out, is busy or is gone. A
hardware backend implements `DdcError::kind` for its own error type (`Other` where it
cannot tell); a simulator may use `Infallible` or `()`, for which hdmi-hal implements it.
Over a `DdcBus`, the transport is `ScdcOverDdc` (below); a transport that is not built on
one, such as a kernel backend over its own SCDC helpers, implements the trait directly.

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

### `DdcBus`

An HDMI connector has one DDC line, an I²C bus shared by several targets: the EDID at
0x50, with the E-DDC segment pointer at 0x30; DisplayID at 0x52; SCDC at 0x54; HDCP at
0x3A and 0x3B; DDC/CI at 0x37. `DdcBus` is one contract for that bus, so code that talks to
one target (SCDC through `ScdcTransport`, an EDID reader, an HDCP implementation) runs over
any hardware that reaches it. The module `ddc` names those addresses as constants.

```rust
pub trait DdcBus {
    type Error: DdcError;

    fn capabilities(&self) -> DdcCapabilities;
    fn write(&self, addr: u8, bytes: &[u8]) -> Result<(), Self::Error>;
    fn write_read(&self, addr: u8, bytes: &[u8], buf: &mut [u8]) -> Result<(), Self::Error>;
    fn read(&self, addr: u8, buf: &mut [u8]) -> Result<(), Self::Error>;
    fn read_segmented(
        &self,
        target: EddcTarget,
        segment: u8,
        offset: u8,
        buf: &mut [u8],
    ) -> Result<(), Self::Error>;
}

pub trait DdcError: core::fmt::Debug {
    fn kind(&self) -> DdcErrorKind;
}
```

The hardware below differs widely: general I²C controllers, DDC masters driven by
segment, address and offset registers, engines that can only read EDID, controllers shared
with firmware. The trait's documentation holds the full contract; the decisions behind it:

**Four operations, one of them E-DDC.** `write`, `write_read` and `read` reach any 7-bit
address, as embedded-hal's operations of the same names do, so an implementation over an
embedded-hal `I2c` maps them directly. `read_segmented` is an E-DDC read of the EDID or
DisplayID structure (VESA E-DDC 1.3): for segment 1 and higher, the segment written to the
pointer at 0x30, then the offset and the read at the target, with no STOP in between, since
the display resets the pointer on STOP or NACK (2.2.5, 8.1.1). That spans two addresses, so
it cannot be expressed as embedded-hal's one-address transaction, and it is the one read
an EDID-only engine can do; it is its own operation so that every kind of hardware can
offer it. For segment 0 an implementation need not write the pointer, and a display
without one still answers (8.1.2); a missing acknowledge of the pointer value for a higher
segment fails the read (6.4, 6.5). `EddcTarget` (`Edid`, `DisplayId`) names the target
rather than taking an address: E-DDC defines exactly these two.

**One call is one transaction; nothing spans calls.** `write`, `write_read` and `read` are
each one combined transfer or an error, never split, as `ScdcTransport::read_block`
promises. `read_segmented` may be split into several E-DDC transactions, each with its own
pointer write, since EDID engines and DDC masters read in pieces; an implementation that
splits says so in its capabilities. A split read can span a hotplug and return a block made
of two EDIDs; the block checksum is the check. Between two calls, another user's
transaction may run: nothing holds the bus across calls, so a procedure with a timing
window across several transactions (HDCP's locality check) cannot rely on `DdcBus` for it;
hardware that can guarantee it does so below, in its HDCP engine.

**Shared by reference.** Every method takes `&self`, because the bus is shared by nature:
four or five users, often in different crates and tasks. The implementation serializes
concurrent calls, one transaction at a time, and never panics doing so; one that cannot
serialize returns an error for the second call and says so. Users hold a handle to the bus:
code over `DdcBus` is generic over `H: Deref<Target: DdcBus>`, so it takes `&B`,
`&'static B`, `Rc<B>` or `Arc<B>` alike, with no `alloc` needed. There are no forwarding
implementations (`impl DdcBus for &B`): in `hdmi-hal-async` one would conflict with the
`Send` twin's blanket implementation, and the sync crate keeps the same shape. The
alternative, `&mut self` with bus-sharing wrappers as embedded-hal has, was rejected: every
user would need a wrapper whose exclusion is per call anyway, and OS backends already
serialize.

**Arguments are checked before any bus activity.** An address outside I²C's reserved
ranges (0x00–0x07, 0x78–0x7F; `is_usable_address`), a segment above 7Fh (`MAX_SEGMENT`),
`offset + buf.len()` past a segment's 256 bytes, and a `write` of no bytes (I²C's address
probe, which many adapters reject) are `Unsupported`, never a panic. An empty `buf` is `Ok`
with no bus activity; `write_read` with no bytes to write is a `read`.

**Errors carry a kind.** `DdcError::kind` gives a `DdcErrorKind`: `NoAcknowledge` (from the
address, a data byte, or unknown: `NoAcknowledgeSource`), `Timeout`, `Busy`, `Unsupported`,
`Detached` (the bus is gone, not transient) or `Other`. `NoAcknowledge` usually means
nothing answers at the address; `Timeout`, `Busy` and `Other` may be transient; `Detached`
and `Unsupported` are not. `Unknown` and `Other` keep the bound cheap for hardware that
reports one failure for everything. `DdcError` is implemented for `Infallible`, `()` and
`DdcErrorKind` itself. An implementation adds no retries of its own, since retry counts
are the caller's policy and would multiply with it; retries in the layer below it are
named in its documentation. After an error the bus is usable for the next call as far as
the hardware allows: the implementation recovers it or says what is needed.

**Capabilities report what the hardware can do, and never more.** `DdcCapabilities` says,
per operation, the longest transfer (`0`: not available), which addresses are reachable
(a set of the 128 seven-bit addresses), which E-DDC targets and the highest segment
`read_segmented` reads, the piece size it splits into, whether `write_read` and `read` are
split on the wire, and the clock-stretch timeout. What an implementation cannot vouch for,
it reports as unavailable, and the operation returns `Unsupported` before it reaches the
bus: a report may under-state, never over-state. The type has private fields, built with
`const fn` constructors (`general()`, everything reachable; `none()`, nothing) and `with_…`
setters, and read through accessors, so implementations outside hdmi-hal can build it and
fields can be added later. The report is for planning: choosing how to read the EDID,
checking an HDCP message fits, explaining a gap in a diagnostic.

**Not in it:** the sink's Read Request, an SCDC read without an offset, and a bus lock
across calls; none of the reference implementations needs them.

#### `ScdcOverDdc`

`scdc::ScdcOverDdc<H>` is the `ScdcTransport` over a `DdcBus`, for any handle
`H: Deref<Target: DdcBus>`: `read` and `read_block` are one `write_read` at 0x54 each,
`write` one `write`, and its error is the bus's. It is glue between two of this crate's own
traits, which is why it lives here rather than in a platform crate, and a type rather than a
blanket implementation, which would conflict with `hdmi-hal-async`'s blanket for
`SendScdcTransport`.

Construction refuses a bus that cannot serve `ScdcTransport`: `new(bus)` returns the handle
when 0x54 is unreachable or `write_read` cannot carry a one-byte offset and a 16-byte read
(`read_block`'s minimum); `bytewise(bus)` needs only one-byte reads, and reads blocks one
register per transaction, `read_block`'s documented exception.

---

### PHY traits by link mode

The PHY contract is split by link mode, so hardware implements only what it has: `FrlPhy`
for FRL, `TmdsPhy` for TMDS, and `PhyPower` for power, shared by both. A TMDS-only PHY
implements nothing of FRL; a PHY that is always on implements no power trait. On a PHY
implementing both mode traits, **entering one mode leaves the other**: `TmdsPhy::configure`
leaves FRL, `FrlPhy::set_frl_rate` with a rate leaves TMDS, and
`set_frl_rate(HdmiForumFrl::NotSupported)` turns FRL off and enters nothing, as
`TmdsPhy::stop_output` on a PHY in FRL does. The rule is
stated in both mode traits' documentation and in `PhyPower`'s.

### `FrlPhy`

Covers what FRL link training needs from an HDMI 2.1 transmitter PHY: FRL rate
selection, link training patterns, FRL output mode, and each lane's TxFFE level (FRL's
transmitter equalization, a combination of pre-shoot and de-emphasis). Further PHY
configuration is not covered yet; see "Not covered yet" below.

```rust
pub trait FrlPhy {
    type Error: core::fmt::Debug;

    /// Select the FRL rate, or turn FRL off. Triggers the required lane reconfiguration
    /// sequence.
    fn set_frl_rate(&mut self, rate: HdmiForumFrl) -> Result<(), Self::Error>;

    /// Drive the given link training patterns on the physical lanes, one per lane.
    fn send_ltp(&mut self, patterns: LanePatterns) -> Result<(), Self::Error>;

    /// Select what the transmitter sends on the FRL lanes: gap characters only, or video,
    /// data islands and control periods.
    fn set_frl_output(&mut self, output: FrlOutput) -> Result<(), Self::Error>;

    /// Adjust equalization parameters after link training feedback.
    fn adjust_equalization(&mut self, params: EqParams) -> Result<(), Self::Error>;
}
```

`set_frl_rate` returns once the PHY transmits at the rate, after whatever bring-up its
hardware needs. `HdmiForumFrl::NotSupported` turns FRL off and enters no other mode:
entering TMDS is `TmdsPhy::configure`'s. Before the split, the trait was `HdmiPhy`, it
carried `set_scrambling`, and `NotSupported` meant "switch to TMDS"; the scrambler is the
source's TMDS scrambler, so it moved to `TmdsPhy`.

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
full FFE and ignores no FFE), and no public source settles it. A PHY that
cannot restrict its TxFFE may ignore them and says so. The Rockchip driver applies them
only after the sink requests the DDE compliance pattern (LTP 4) on a lane, the AMD driver
with every TxFFE setting. Every `adjust_equalization` call carries the complete settings,
restrictions included.

`set_frl_output` takes `FrlOutput`: `GapOnly` (gap characters only, during link training
and while waiting for the sink to start FRL) or `Active` (video, data islands and control
periods, set once training has succeeded). It is part of `FrlPhy` because both the link
training crate and the integration layer drive it, and because `FrlPhy` already carries
link-level operations such as `send_ltp`.

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

**Not covered yet.** Today's surface is what link training and TMDS bring-up need. The
mode-setting layer and platform bring-up will also need lane mapping (lane order and
polarity, for both mode traits), TMDS pre-emphasis and swing, spread-spectrum clocking and
power states beyond off and on. Following stack-ordered
delivery (see "Design Principles"), each is decided when the layer that needs it is built,
including whether it belongs in the trait or in a backend's own configuration: lane
mapping, for example, is fixed by the board and may be set when the backend is
constructed. New trait methods with default implementations are not breaking changes;
required ones would be. `doc/roadmap.md` tracks them.

---

### `TmdsPhy`

Covers what TMDS bring-up and tear-down need from the transmitter PHY, and nothing more.
Each method is a step of those procedures: stop the output, configure, set the scrambler,
start the output; tear-down and a quick restore set the scrambler alone, with the output
running.

```rust
pub enum TmdsClockRatio { Div10, Div40 }

#[non_exhaustive]
pub struct TmdsPhyConfig {
    pub character_rate_khz: u32,   // rounded up, never truncated
    pub clock_ratio: TmdsClockRatio,
    pub video: TmdsVideo,
}

#[non_exhaustive]
pub struct TmdsVideo {
    pub pixel_clock_khz: u32,      // at the encoder: repetition included, deep colour not
    pub color_depth: ColorBitDepth,
    pub color_format: ColorFormat,
    pub pixel_repetition: u8,      // 0: none
}

#[non_exhaustive]
pub struct TmdsPhyCapabilities {
    pub max_character_rate_khz: u32,
    pub scrambling: bool,
    pub low_rate_scrambling: bool, // counts only with `scrambling`
}

pub trait TmdsPhy {
    type Error: core::fmt::Debug;

    fn capabilities(&self) -> TmdsPhyCapabilities;
    fn configure(&mut self, config: TmdsPhyConfig) -> Result<(), Self::Error>;
    fn set_scrambling(&mut self, enabled: bool) -> Result<(), Self::Error>;
    fn start_output(&mut self) -> Result<(), Self::Error>;
    fn stop_output(&mut self) -> Result<(), Self::Error>;
}
```

The value types are built with `const fn` constructors and setters (`new`, `with_…`);
`TmdsPhyCapabilities::new` starts with no scrambler, so a capability is never reported
that was not stated. The trait's documentation holds the full contract; in short:

- **`configure`** sets the character rate and clock ratio with the output stopped, and
  leaves it stopped. It gets the video as well: some PHYs are set up from the pixel clock
  and colour depth rather than the character rate (the Synopsys DesignWare HDMI PHY's
  tables are keyed by pixel clock, with a column per depth), and one character rate comes
  from several pixel clocks. A PHY keyed by the rate ignores the rest. `pixel_clock_khz` is the
  clock at which pixels reach the TMDS encoder, pixel repetition included and the
  deep-colour factor not (27 000 kHz for 720(1440)x480i). The convention is chosen and
  stated because Linux drivers read a repeated mode's clock both ways: i915 doubles it for
  the port, dw-hdmi takes the mode's clock and refuses double-clocked modes (the field's
  documentation cites both). The caller guarantees that `character_rate_khz` is the rate
  `video` gives, rounded up, so a PHY may configure itself from either. Further inputs go
  into `TmdsVideo`, which is `#[non_exhaustive]`, so adding one is not breaking. An
  implementation that cannot reconfigure while transmitting returns an error when called
  with the output running; one that can stops the output first, so the output is stopped
  whenever `configure` succeeds. On a powered-off PHY it returns an error. The scrambler state afterwards is unspecified (kept,
  reset or set from the rate, as the implementation does): callers set it with
  `set_scrambling` after `configure`.
- **`set_scrambling`** turns the source's scrambler on or off, with the output running or
  stopped. `set_scrambling(false)` succeeds on a PHY without a scrambler, since plain links
  call it; callers never ask one to scramble, and `set_scrambling(true)` on one returns an
  error.
- **`start_output`** returns once the PHY transmits at the configured rate, after whatever
  its hardware needs (a PLL lock, a reset sequence): callers time from its return. Without
  a successful `configure` since the last power-on, it returns an error.
- **`stop_output`** stops whatever the PHY transmits, FRL included on a PHY still in FRL.
  Callable in any state, powered off included; stopping a stopped output is not an error.
  It keeps the PHY powered and its TMDS configuration. A PHY in FRL leaves FRL, as
  `set_frl_rate(NotSupported)` does: nothing in `TmdsPhy` could resume FRL output, so a
  PHY left in FRL with its output stopped would be a state no TMDS caller can use.
- **`capabilities`** is a plain report, neither fallible nor `async`. `scrambling` is a
  field of its own rather than derived from the ceiling: a link at exactly 340 000 kHz may
  be scrambled, so a PHY whose ceiling is 340 Mcsc and which has no scrambler must be able
  to say so. Hardware that cannot do something reports it as a capability.

**Errors** are `core::fmt::Debug`, so generic code can format them, with no classification:
the procedures treat every failed PHY call alike, so a kind would be read by nobody.

**Not in it:** electrical settings (swing, pre-emphasis, lane mapping), which are the
implementation's; the video pipeline beyond what `TmdsVideo` gives the PHY; hotplug and RX
sense, which are inputs to the caller; and power, which is `PhyPower`'s.

---

### `PhyPower`

Powering the PHY off and on, shared by `TmdsPhy` and `FrlPhy`.

```rust
pub trait PhyPower {
    type Error: core::fmt::Debug;

    fn power_on(&mut self) -> Result<(), Self::Error>;
    fn power_off(&mut self) -> Result<(), Self::Error>;
}
```

- **`power_off`** stops any output and leaves no mode: afterwards the PHY holds no TMDS
  configuration and no FRL rate, so a bring-up must configure it again. Callable in any
  state; powering off a PHY that is off is not an error.
- **`power_on`** leaves the output stopped and no mode entered; the caller then enters one,
  with `TmdsPhy::configure` or `FrlPhy::set_frl_rate`. On a PHY that is already on it does
  nothing: not an error, and neither the mode nor the output changes.
- **`stop_output` is not `power_off`:** it keeps the configuration, so a bring-up after it
  starts from the state the procedures expect.

**A trait of its own, not a supertrait** of the mode traits. Link training and TMDS
bring-up bound on a mode trait alone and never change the power; the caller that owns the
link powers the PHY, on unplug and before a bring-up, and bounds on `PhyPower` as well. A
supertrait would make every mode-trait implementor write power methods that a PHY which is
always on cannot honour. One PHY implementing several of these traits may use one error
type for all of them.

---

### `Clock` and `Delay` (module `time`)

What procedures need from time: a clock to stamp events and check deadlines, and a delay
to wait between steps and polls. Both plug into the platform's counter and timer.

```rust
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Timestamp(u64); // microseconds from an arbitrary monotonic epoch

pub trait Clock {
    fn now(&self) -> Timestamp;
}

pub trait Delay {
    fn delay(&mut self, duration: core::time::Duration);
}
```

- **`Timestamp`** is a type of its own rather than a bare `u64`, so microseconds are not
  mixed with other counts; no platform `Instant` is assumed, and `u64` microseconds do not
  wrap in any lifetime a device has. `from_micros`, `as_micros`,
  `checked_duration_since`, `saturating_duration_since` and `checked_add`, all `const fn`.
  `checked_add` rounds a duration that is not a whole number of microseconds up, so a
  deadline is never earlier than asked. Durations are `core::time::Duration`.
- **`Clock::now`** is neither fallible nor `async`: reading a counter does not wait. It
  takes `&self`, so one clock can be shared; an implementation with state uses interior
  mutability. It never goes backwards; only differences mean anything. Resolution is the
  implementation's, and it documents it.
- **`Delay::delay`** waits at least the duration, rounding up to its own tick, never less,
  so a minimum wait holds whatever the timer. It is measured on the platform's `Clock`:
  `now()` after `delay(d)` is at least `d` later than before it, so a test double whose
  delay does not advance its clock breaks the contract. It is infallible: a timer that cannot wait
  leaves the caller nothing to recover with.

**Two traits, not one:** the clock is read where nothing waits (stamping an event, checking
a deadline), the async delay is `async` while the clock is not, and a platform may take
them from different peripherals. A test double may implement both on one type, advancing
its clock by each delay.

**The stack's own `Delay`, not embedded-hal's `DelayNs`:** generic async code bounded by
embedded-hal-async's `DelayNs` cannot produce a `Send` future, and that crate has no `Send`
twin; `hdmi-hal-async` gives `Delay` one, as it does every trait. An implementation over a
`DelayNs` is a few lines, shown in the `time` module's documentation; no adapter type ships
here, since it would add a dependency and be glue to another crate's trait.

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

- **Trait shapes are identical.** `hdmi-hal-async::ScdcTransport`,
  `hdmi-hal-async::phy::FrlPhy` and the others mirror their sync counterparts exactly; only
  method signatures gain `async`. Plain reports stay plain functions
  (`TmdsPhy::capabilities`), and `Clock` is re-exported as it is: reading a counter does
  not wait. No behavioral differences, no extra associated types.
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

- **Contracts only.** No hardware, platform or clock implementation lives in this crate,
  and no policy. The moment one appears here, it becomes a platform assumption baked into a
  foundational dependency. The exception is glue between this crate's own traits
  (`ScdcOverDdc`, `read_block_bytewise`), which assumes nothing about a platform.
- **Accuracy and completeness.** Trait surfaces must not omit operations the standard
  defines. A method left out because it seems unlikely to be needed now is a missing
  contract that forces a breaking change or a fork later. What callers choose to use is
  their decision; what the standard defines is the scope.
- **Stack-ordered delivery.** The 0.1.0 release scope covers what the next layer in the
  stack needs to function. Everything else the standard defines still gets built, but is
  tracked on the roadmap until the layer that needs it is being implemented. This is a
  sequencing decision, not a judgement about what is worth implementing.
- **Raw boundaries, typed protocols.** `ScdcTransport` is intentionally byte-level.
  The typed SCDC register map belongs in the protocol crate. Mixing them would encode
  HDMI protocol knowledge into what should be a pure I/O contract.
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
  Verify with `gh attestation verify <file> --repo ashlar-display/hdmi-hal` (releases from
  before the move to the `ashlar-display` organization: see "Verifying releases" in the
  README).
