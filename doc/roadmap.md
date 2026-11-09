# Roadmap

What each release contains is in the [changelog](../CHANGELOG.md); this page is about
what comes next.

## Planned

### PHY traits by link mode (breaking)

The next breaking release splits the PHY contract by link mode, so TMDS-only hardware
implements only what it has, and adds the time the procedures driving it need. The traits
are on the development branch, described in the [architecture](architecture.md), and are
being proven by prototypes of the TMDS bring-up procedures and a simulated sink before the
release fixes them:

- **`HdmiPhy` is renamed `FrlPhy`**, with no alias. `set_frl_rate(HdmiForumFrl::NotSupported)`
  then means "FRL off" rather than "switch to TMDS"; entering TMDS is `TmdsPhy`'s.
- **A new `TmdsPhy`:** the TMDS character rate and the 1/10 or 1/40 clock ratio, with the
  video (`TmdsVideo`) for PHYs set up from the pixel clock; starting and stopping TMDS
  output; the source's TMDS scrambler (`set_scrambling` moves here); and a capabilities
  report, including whether the source has a scrambler and whether it can scramble at or
  below 340 Mcsc.
- **`PhyPower`:** power off and on, used on unplug and before bring-up, a trait of its own
  rather than a supertrait of the mode traits. On a PHY implementing both mode traits,
  entering one mode leaves the other.
- **The `time` module:** `Timestamp` (microseconds from an arbitrary monotonic epoch),
  `Clock` (a monotonic `now()`) and `Delay`, which plug into the hardware's counter and
  timer.
- **`ddc::DdcBus`, one trait for the DDC bus.** EDID (0x50, with the E-DDC segment pointer
  at 0x30), DisplayID (0x52), SCDC (0x54), HDCP (0x3A, 0x3B) and DDC/CI (0x37) share one bus
  per connector. `DdcBus` offers `write`, `write_read` and `read` on any 7-bit address, and
  `read_segmented` for E-DDC reads of EDID and DisplayID (VESA E-DDC 1.3), which span two
  addresses and are the one read EDID-only hardware can do. All methods take `&self`; one call
  is one transaction, and the implementation serializes concurrent calls.
  - **Errors** carry a kind (`DdcError::kind()`): no acknowledgement (from the address, a data
    byte, or unknown), timeout, busy, unsupported, detached, other.
  - **Capabilities** say what the hardware can do and never more: per-operation length limits,
    reachable addresses, the highest E-DDC segment, whether reads are split on the wire.
  - **`scdc::ScdcOverDdc`** implements `ScdcTransport` over a `DdcBus`, generic over any handle
    that dereferences to the bus (`&B`, `Rc<B>`, `Arc<B>`), so several users share one bus.
    It is a type rather than a blanket impl, which would conflict with hdmi-hal-async's
    `SendScdcTransport` blanket.
  - **`ScdcTransport::Error`** gets the same `DdcError` bound, implemented for `Infallible`
    and `()`.
  - The crate stays free of hardware, platform and clock implementations and of policy; it
    holds traits and the glue between them (`read_block_bytewise`, the adapter).
- **A cancellation clause** for the async PHY, power and delay traits' futures in
  `hdmi-hal-async`, as `ScdcTransport` has.

Every crate that re-exports hdmi-hal's types follows with a breaking release.

### Further PHY configuration

Decided when the layer that needs it is built; as trait methods with default
implementations they are not breaking changes, as required methods they would be:

- **Lane mapping** — lane order and polarity, for both mode traits. Fixed by the board, so
  it may belong in a backend's configuration at construction rather than in the traits.
- **TMDS pre-emphasis and swing** — today the implementation's own settings, outside
  `TmdsPhy`; whether they move into the trait or stay in the backend's configuration is
  decided when the mode-setting layer needs them, as for lane mapping. FRL's transmitter
  equalization is already each lane's TxFFE level in `EqParams`.
- **Spread-spectrum clocking.**
- **Power states beyond off and on** — standby, partial power.

### CEC traits

Two levels, because CEC hardware comes at both: a message-level trait (as Linux's
`/dev/cecN` and most CEC controllers, which do the bit timing in hardware, provide), and a
bit-level line trait (bit-bang read, write, collision detection) for firmware that drives the
line itself, with an adapter that implements the message level over it. To be defined when
CEC implementation begins. The bar for inclusion here is that more than one crate needs the
contract and that it abstracts hardware.
