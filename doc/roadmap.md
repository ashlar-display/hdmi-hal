# Roadmap

What each release contains is in the [changelog](../CHANGELOG.md); this page is about
what comes next.

## Planned

### PHY traits by link mode (breaking)

`HdmiPhy` covers the FRL PHY. A planned breaking release splits the PHY contract by link
mode, so TMDS-only hardware implements only what it has:

- **`HdmiPhy` is renamed `FrlPhy`**, with no alias. `set_frl_rate(HdmiForumFrl::NotSupported)`
  then means "FRL off" rather than "switch to TMDS"; entering TMDS is `TmdsPhy`'s.
- **A new `TmdsPhy`:** the TMDS character rate and the 1/10 or 1/40 clock ratio, starting
  and stopping TMDS output (which the bring-up order and the suspend during a ratio change
  need), the source's TMDS scrambler (`set_scrambling` moves here), and a capabilities
  report, including whether the source can scramble at or below 340 Mcsc.
- **A small power trait:** power off and on, used on unplug and before bring-up. On a PHY
  implementing both mode traits, entering one mode leaves the other.
- **Clock and delay traits:** a monotonic `now()` and a delay, which plug into the
  hardware's timer, for the procedures that drive these traits.
- **`DdcBus`:** EDID (0x50/0x30), SCDC (0x54) and HDCP (0x74) share one bus; a `DdcBus`
  contract comes as an adapter type, not a blanket `ScdcTransport` impl (that would
  conflict with hdmi-hal-async's `SendScdcTransport` blanket). Designed in its own pass.
- **A cancellation clause** for the PHY traits' futures, as `ScdcTransport` has.

Every crate that re-exports hdmi-hal's types follows with a breaking release.

### Further PHY configuration

Decided when the layer that needs it is built; as trait methods with default
implementations they are not breaking changes, as required methods they would be:

- **Lane mapping** — lane order and polarity, for both mode traits. Fixed by the board, so
  it may belong in a backend's configuration at construction rather than in the traits.
- **TMDS pre-emphasis and swing** — on `TmdsPhy`; FRL's transmitter equalization is
  already each lane's TxFFE level in `EqParams`.
- **Spread-spectrum clocking.**
- **Power states beyond off and on** — standby, partial power.

### CEC traits

Two levels, because CEC hardware comes at both: a message-level trait (as Linux's
`/dev/cecN` and most CEC controllers, which do the bit timing in hardware, provide), and a
bit-level line trait (bit-bang read, write, collision detection) for firmware that drives the
line itself, with an adapter that implements the message level over it. To be defined when
CEC implementation begins. The bar for inclusion here is that more than one crate needs the
contract and that it abstracts hardware.
