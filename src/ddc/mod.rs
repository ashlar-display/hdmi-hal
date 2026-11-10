//! The DDC bus: one trait for every target on it.
//!
//! An HDMI connector carries one DDC line, an I²C bus, shared by several targets: the
//! sink's EDID at [`EDID`] (with the E-DDC segment pointer at [`SEGMENT_POINTER`]), its
//! DisplayID at [`DISPLAYID`], SCDC at [`SCDC`], HDCP at [`HDCP_PRIMARY`] and
//! [`HDCP_SECONDARY`], and DDC/CI at [`DDC_CI`]. [`DdcBus`] is the contract for that bus;
//! code that talks to one target (SCDC through `ScdcTransport`, an EDID reader, an HDCP
//! implementation) runs over it.
//!
//! The hardware below differs widely: general I²C controllers, DDC masters driven by
//! segment, address and offset registers, engines that can only read EDID, controllers
//! shared with firmware. [`DdcCapabilities`] says what an implementation can do, and an
//! operation it cannot do fails with [`DdcErrorKind::Unsupported`] before it reaches the bus.

use core::convert::Infallible;

/// The E-DDC segment pointer (8-bit address 60h), written before an EDID or DisplayID read
/// of segment 1 or higher (VESA E-DDC 1.3, 2.2.5).
pub const SEGMENT_POINTER: u8 = 0x30;

/// HDCP's primary link port (8-bit address 74h; VESA E-DDC 1.3, Table 2-2; Linux's
/// `DRM_HDCP_DDC_ADDR` and amdgpu's primary port, torvalds/linux `e774d5f1b`,
/// `include/drm/display/drm_hdcp.h:30`, `drivers/gpu/drm/amd/display/dc/hdcp/hdcp_msg.c:128`).
pub const HDCP_PRIMARY: u8 = 0x3A;

/// HDCP's secondary link port (8-bit address 76h; VESA E-DDC 1.3, Table 2-2; amdgpu's
/// secondary port, torvalds/linux `e774d5f1b`, `drivers/gpu/drm/amd/display/dc/hdcp/hdcp_msg.c:129`).
pub const HDCP_SECONDARY: u8 = 0x3B;

/// DDC/CI, the display's command interface (8-bit address 6Eh; VESA DDC/CI 1.1, 2.3).
pub const DDC_CI: u8 = 0x37;

/// The EDID structure (8-bit address A0h; VESA E-DDC 1.3, 2.2.1).
pub const EDID: u8 = 0x50;

/// The DisplayID structure (8-bit address A4h; VESA E-DDC 1.3, 2.2.1).
pub const DISPLAYID: u8 = 0x52;

/// SCDC, the status and control data channel (8-bit address A8h; Linux's SCDC helper and
/// amdgpu, torvalds/linux `e774d5f1b`, `drivers/gpu/drm/display/drm_scdc_helper.c:56`,
/// `drivers/gpu/drm/amd/display/dc/dc_hdmi_types.h:58`).
pub const SCDC: u8 = 0x54;

/// The highest E-DDC segment (VESA E-DDC 1.3, Tables 5-1 and 5-2: 00h–7Fh).
pub const MAX_SEGMENT: u8 = 0x7F;

/// Whether `addr` is a 7-bit address a [`DdcBus`] may be asked to reach: not above 0x7F,
/// and outside I²C's reserved ranges 0x00–0x07 and 0x78–0x7F.
///
/// An implementation returns [`DdcErrorKind::Unsupported`], without bus activity, for an
/// address this rejects.
pub const fn is_usable_address(addr: u8) -> bool {
    addr >= 0x08 && addr <= 0x77
}

/// Access to the DDC bus of one connector.
///
/// # Operations
///
/// | Method | On the wire |
/// |---|---|
/// | [`write`](Self::write) | START, address + W, `bytes`, STOP |
/// | [`write_read`](Self::write_read) | START, address + W, `bytes`, repeated START, address + R, `buf.len()` bytes, STOP |
/// | [`read`](Self::read) | START, address + R, `buf.len()` bytes, STOP |
/// | [`read_segmented`](Self::read_segmented) | an E-DDC read: for segment 1 and higher a write of the segment to [`SEGMENT_POINTER`], then as `write_read` with the offset to the target, with no STOP in between; for segment 0, no write to the pointer |
///
/// Register-style targets (SCDC, HDCP) take a register offset as the first byte of
/// `bytes`.
///
/// # One call, one transaction
///
/// [`write`](Self::write), [`write_read`](Self::write_read) and [`read`](Self::read) each
/// run as one transaction or return an error: an implementation never splits them into
/// several transfers. Where the hardware does so anyway (one byte per hardware
/// operation), the implementation says so through [`DdcCapabilities::splits_reads`].
///
/// [`read_segmented`](Self::read_segmented) may be split into several E-DDC transactions,
/// each writing the segment pointer and the offset again (the display resets the pointer
/// on STOP or NACK: VESA E-DDC 1.3, 2.2.5, 8.1.1); an implementation that splits reports
/// its piece size through [`DdcCapabilities::segmented_piece`]. A split read can span a
/// hotplug and return a block made of two EDIDs: the block checksum is the check.
///
/// Between two calls, another user's transaction may run: nothing here holds the bus
/// across calls.
///
/// # Sharing
///
/// Every method takes `&self`, so several users (an EDID reader, an SCDC client, an HDCP
/// implementation) share one bus through references or shared pointers. The
/// implementation serializes concurrent calls, one transaction at a time, and never panics
/// doing so; one that cannot serialize returns an error for the second call and says so in
/// its documentation.
///
/// # Arguments
///
/// Checked before any bus activity:
///
/// - `addr` outside [`is_usable_address`]: [`DdcErrorKind::Unsupported`].
/// - An empty `buf` (in `read`, `write_read`, `read_segmented`): `Ok`, no bus activity.
/// - `write_read` with empty `bytes` is a [`read`](Self::read).
/// - `write` with empty `bytes`: [`DdcErrorKind::Unsupported`] (a zero-length write is
///   I²C's address probe, which many adapters reject).
/// - `read_segmented` with `segment` above [`MAX_SEGMENT`], or `offset + buf.len()` above
///   256: [`DdcErrorKind::Unsupported`].
/// - Anything [`capabilities`](Self::capabilities) rules out (a length, an address, a
///   target, a segment): [`DdcErrorKind::Unsupported`].
///
/// # E-DDC
///
/// For segment 1 and higher, a missing acknowledge of the pointer value fails the read: the
/// display has no such segment (VESA E-DDC 1.3, 6.4, 6.5). For segment 0 an implementation
/// does not need to write the pointer, and a display without one still answers (8.1.2);
/// hardware whose engine writes it anyway conforms all the same. A read across the
/// 128-byte block boundary is allowed, but whether the display continues past it is the
/// display's (2.2.6).
///
/// # Errors
///
/// Every error carries a [`DdcErrorKind`]. An implementation adds no retries of its own;
/// retries in the layer below it (an OS bus layer, a fallback in the driver) are named in
/// its documentation, since they multiply with the caller's. After an error the bus is
/// usable for the next call as far as the hardware allows: the implementation recovers it
/// (clock pulses, an abort, a reset) or says what is needed. The contents of `buf` after
/// an error are unspecified.
///
/// # Example
///
/// A bus that reaches nothing but an EDID store, as an EDID-only engine does:
///
/// ```
/// use hdmi_hal::ddc::{DdcBus, DdcCapabilities, DdcError, DdcErrorKind, EddcTarget};
///
/// #[derive(Debug)]
/// struct Unsupported;
///
/// impl DdcError for Unsupported {
///     fn kind(&self) -> DdcErrorKind {
///         DdcErrorKind::Unsupported
///     }
/// }
///
/// /// Two blocks of EDID, segment 0 only.
/// struct EdidEngine {
///     edid: [u8; 256],
/// }
///
/// impl DdcBus for EdidEngine {
///     type Error = Unsupported;
///
///     fn capabilities(&self) -> DdcCapabilities {
///         DdcCapabilities::none()
///             .with_eddc_target(EddcTarget::Edid, true)
///             .with_max_segment(Some(0))
///     }
///
///     fn write(&self, _addr: u8, _bytes: &[u8]) -> Result<(), Unsupported> {
///         Err(Unsupported)
///     }
///
///     fn write_read(&self, _addr: u8, _bytes: &[u8], _buf: &mut [u8]) -> Result<(), Unsupported> {
///         Err(Unsupported)
///     }
///
///     fn read(&self, _addr: u8, _buf: &mut [u8]) -> Result<(), Unsupported> {
///         Err(Unsupported)
///     }
///
///     fn read_segmented(
///         &self,
///         target: EddcTarget,
///         segment: u8,
///         offset: u8,
///         buf: &mut [u8],
///     ) -> Result<(), Unsupported> {
///         let start = usize::from(offset);
///         if target != EddcTarget::Edid || segment != 0 || start + buf.len() > 256 {
///             return Err(Unsupported);
///         }
///         buf.copy_from_slice(&self.edid[start..start + buf.len()]);
///         Ok(())
///     }
/// }
///
/// let bus = EdidEngine { edid: [0; 256] };
/// let mut header = [0u8; 8];
/// bus.read_segmented(EddcTarget::Edid, 0, 0, &mut header).unwrap();
/// assert!(!bus.capabilities().reaches(hdmi_hal::ddc::SCDC));
/// ```
pub trait DdcBus {
    /// The error type, classified by [`DdcError::kind`].
    type Error: DdcError;

    /// What this implementation can do. Never more than it can: what it cannot vouch for,
    /// it reports as unavailable.
    fn capabilities(&self) -> DdcCapabilities;

    /// Write `bytes` to `addr` in one transaction.
    fn write(&self, addr: u8, bytes: &[u8]) -> Result<(), Self::Error>;

    /// Write `bytes` to `addr`, then read `buf.len()` bytes from it after a repeated START,
    /// in one transaction.
    fn write_read(&self, addr: u8, bytes: &[u8], buf: &mut [u8]) -> Result<(), Self::Error>;

    /// Read `buf.len()` bytes from `addr` in one transaction, with no write before it.
    fn read(&self, addr: u8, buf: &mut [u8]) -> Result<(), Self::Error>;

    /// Read `buf.len()` bytes of `target`'s structure from `segment`, starting at `offset`
    /// within it: an E-DDC read (see "E-DDC" above).
    fn read_segmented(
        &self,
        target: EddcTarget,
        segment: u8,
        offset: u8,
        buf: &mut [u8],
    ) -> Result<(), Self::Error>;
}

/// An error of a [`DdcBus`], classified.
///
/// Implemented for [`Infallible`] and `()` (kind [`DdcErrorKind::Other`]), so test doubles
/// can use either, and for [`DdcErrorKind`] itself.
pub trait DdcError: core::fmt::Debug {
    /// What kind of failure this is.
    fn kind(&self) -> DdcErrorKind;
}

/// The kind of a [`DdcBus`] failure.
///
/// What a kind means to a caller: [`NoAcknowledge`](Self::NoAcknowledge) usually means
/// nothing answers at that address, not always (a passive adapter may refuse a first
/// transaction); [`Timeout`](Self::Timeout), [`Busy`](Self::Busy) and
/// [`Other`](Self::Other) may be transient; [`Unsupported`](Self::Unsupported) and
/// [`Detached`](Self::Detached) are not.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub enum DdcErrorKind {
    /// A byte was not acknowledged.
    NoAcknowledge(NoAcknowledgeSource),
    /// The transaction did not complete in time: a clock held low too long, a stuck bus,
    /// or the implementation's own limit.
    Timeout,
    /// The bus or the controller is held by someone else: lost arbitration, another
    /// master, firmware or a hardware engine.
    Busy,
    /// An operation, address, target, segment or length this implementation cannot do,
    /// refused before bus activity.
    Unsupported,
    /// The bus is gone: its adapter removed, the device unplugged.
    Detached,
    /// Any other failure, including one the implementation cannot classify.
    Other,
}

/// Which byte of a transaction was not acknowledged.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub enum NoAcknowledgeSource {
    /// The address byte: usually, nothing answers at that address.
    Address,
    /// A data byte.
    Data,
    /// Not known: many controllers do not report which byte it was.
    Unknown,
}

/// The structures an E-DDC read reaches, each with its own address and the shared segment
/// pointer (VESA E-DDC 1.3, 2.2.1, 2.2.4).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub enum EddcTarget {
    /// The EDID structure, at [`EDID`].
    Edid,
    /// The DisplayID structure, at [`DISPLAYID`].
    DisplayId,
}

impl EddcTarget {
    /// The target's 7-bit address.
    pub const fn address(self) -> u8 {
        match self {
            EddcTarget::Edid => EDID,
            EddcTarget::DisplayId => DISPLAYID,
        }
    }

    const fn bit(self) -> u8 {
        match self {
            EddcTarget::Edid => 0b01,
            EddcTarget::DisplayId => 0b10,
        }
    }
}

impl DdcError for Infallible {
    fn kind(&self) -> DdcErrorKind {
        match *self {}
    }
}

impl DdcError for () {
    fn kind(&self) -> DdcErrorKind {
        DdcErrorKind::Other
    }
}

/// A kind is an error of its own kind, for an implementation with nothing more to report
/// (as embedded-hal's `ErrorKind` is its own `Error`).
impl DdcError for DdcErrorKind {
    fn kind(&self) -> DdcErrorKind {
        *self
    }
}

/// The addresses [`is_usable_address`] accepts, as a set (bit `n` for address `n`).
const USABLE_ADDRESSES: u128 = {
    let mut set = 0u128;
    let mut addr = 0u8;
    while addr <= 0x7F {
        if is_usable_address(addr) {
            set |= 1u128 << addr;
        }
        addr += 1;
    }
    set
};

/// What a [`DdcBus`] implementation can do.
///
/// It may under-state, never over-state: what an implementation cannot vouch for, it
/// reports as unavailable, and the operation returns [`DdcErrorKind::Unsupported`] before
/// it reaches the bus.
///
/// Built from [`general`](Self::general) or [`none`](Self::none) and the `with_*`
/// setters, all `const`:
///
/// ```
/// use hdmi_hal::ddc::{DdcCapabilities, DDC_CI, SCDC};
///
/// // An SMBus-style adapter: one-byte offsets, 32-byte block reads, single-byte plain reads.
/// const SMBUS: DdcCapabilities = DdcCapabilities::general()
///     .with_write_max(33)
///     .with_write_read_max(1, 32)
///     .with_read_max(1)
///     .with_max_segment(Some(0))
///     .with_segmented_piece(Some(32));
///
/// assert_eq!(SMBUS.write_read_max(), (1, 32));
/// assert!(SMBUS.reaches(SCDC));
///
/// // A controller that cannot reach DDC/CI.
/// let caps = DdcCapabilities::general().with_address_reachable(DDC_CI, false);
/// assert!(!caps.reaches(DDC_CI));
/// ```
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DdcCapabilities {
    write_max: u16,
    write_read_write_max: u16,
    write_read_read_max: u16,
    read_max: u16,
    reachable: u128,
    eddc_targets: u8,
    max_segment: Option<u8>,
    segmented_piece: Option<u16>,
    splits_reads: bool,
    stretch_timeout_us: Option<u32>,
}

impl DdcCapabilities {
    /// Everything: every usable address, no length limit below `u16::MAX`, both E-DDC
    /// targets up to [`MAX_SEGMENT`], reads in one transaction, stretch timeout unknown.
    pub const fn general() -> Self {
        DdcCapabilities {
            write_max: u16::MAX,
            write_read_write_max: u16::MAX,
            write_read_read_max: u16::MAX,
            read_max: u16::MAX,
            reachable: USABLE_ADDRESSES,
            eddc_targets: EddcTarget::Edid.bit() | EddcTarget::DisplayId.bit(),
            max_segment: Some(MAX_SEGMENT),
            segmented_piece: None,
            splits_reads: false,
            stretch_timeout_us: None,
        }
    }

    /// Nothing: no operation, no address, no E-DDC target.
    pub const fn none() -> Self {
        DdcCapabilities {
            write_max: 0,
            write_read_write_max: 0,
            write_read_read_max: 0,
            read_max: 0,
            reachable: 0,
            eddc_targets: 0,
            max_segment: None,
            segmented_piece: None,
            splits_reads: false,
            stretch_timeout_us: None,
        }
    }

    /// The longest [`write`](DdcBus::write), offset byte included; `0`: not available.
    pub const fn with_write_max(mut self, len: u16) -> Self {
        self.write_max = len;
        self
    }

    /// The longest write part (offset byte included) and read part of a
    /// [`write_read`](DdcBus::write_read); `0` in either: not available.
    pub const fn with_write_read_max(mut self, write: u16, read: u16) -> Self {
        self.write_read_write_max = write;
        self.write_read_read_max = read;
        self
    }

    /// The longest [`read`](DdcBus::read); `0`: not available.
    pub const fn with_read_max(mut self, len: u16) -> Self {
        self.read_max = len;
        self
    }

    /// The addresses [`write`](DdcBus::write), [`write_read`](DdcBus::write_read) and
    /// [`read`](DdcBus::read) reach, as a set (bit `n` for address `n`). Addresses
    /// [`is_usable_address`] rejects are removed.
    pub const fn with_reachable_addresses(mut self, set: u128) -> Self {
        self.reachable = set & USABLE_ADDRESSES;
        self
    }

    /// Whether `addr` is reachable; ignored for an address [`is_usable_address`] rejects.
    pub const fn with_address_reachable(mut self, addr: u8, reachable: bool) -> Self {
        if is_usable_address(addr) {
            if reachable {
                self.reachable |= 1u128 << addr;
            } else {
                self.reachable &= !(1u128 << addr);
            }
        }
        self
    }

    /// Whether [`read_segmented`](DdcBus::read_segmented) reaches `target`.
    pub const fn with_eddc_target(mut self, target: EddcTarget, reads: bool) -> Self {
        if reads {
            self.eddc_targets |= target.bit();
        } else {
            self.eddc_targets &= !target.bit();
        }
        self
    }

    /// The highest segment [`read_segmented`](DdcBus::read_segmented) reads (`None`: no
    /// segmented reads; `Some(0)`: segment 0 only); capped at [`MAX_SEGMENT`].
    pub const fn with_max_segment(mut self, max: Option<u8>) -> Self {
        self.max_segment = match max {
            Some(segment) if segment > MAX_SEGMENT => Some(MAX_SEGMENT),
            other => other,
        };
        self
    }

    /// The piece size [`read_segmented`](DdcBus::read_segmented) splits a read into, each
    /// piece its own E-DDC transaction (`None`: one transaction).
    pub const fn with_segmented_piece(mut self, piece: Option<u16>) -> Self {
        self.segmented_piece = piece;
        self
    }

    /// Whether [`write_read`](DdcBus::write_read) and [`read`](DdcBus::read) reach the wire
    /// as several transactions (a byte per hardware operation, for example).
    pub const fn with_splits_reads(mut self, splits: bool) -> Self {
        self.splits_reads = splits;
        self
    }

    /// How long a clock stretch is tolerated before [`DdcErrorKind::Timeout`], in
    /// microseconds (`None`: unknown). DDC/CI 1.1 requires its host to give up after 2 ms
    /// (6.6).
    pub const fn with_stretch_timeout_us(mut self, timeout: Option<u32>) -> Self {
        self.stretch_timeout_us = timeout;
        self
    }

    /// The longest [`write`](DdcBus::write), offset byte included; `0`: not available.
    pub const fn write_max(&self) -> u16 {
        self.write_max
    }

    /// The longest write part and read part of a [`write_read`](DdcBus::write_read).
    pub const fn write_read_max(&self) -> (u16, u16) {
        (self.write_read_write_max, self.write_read_read_max)
    }

    /// The longest [`read`](DdcBus::read); `0`: not available.
    pub const fn read_max(&self) -> u16 {
        self.read_max
    }

    /// The reachable addresses, as a set (bit `n` for address `n`).
    pub const fn reachable_addresses(&self) -> u128 {
        self.reachable
    }

    /// Whether [`write`](DdcBus::write), [`write_read`](DdcBus::write_read) and
    /// [`read`](DdcBus::read) reach `addr`.
    pub const fn reaches(&self, addr: u8) -> bool {
        addr <= 0x7F && self.reachable & (1u128 << addr) != 0
    }

    /// Whether [`read_segmented`](DdcBus::read_segmented) reaches `target`.
    pub const fn reads_target(&self, target: EddcTarget) -> bool {
        self.eddc_targets & target.bit() != 0
    }

    /// The highest segment [`read_segmented`](DdcBus::read_segmented) reads (`None`: no
    /// segmented reads).
    pub const fn max_segment(&self) -> Option<u8> {
        self.max_segment
    }

    /// The piece size of a split [`read_segmented`](DdcBus::read_segmented) (`None`: one
    /// transaction).
    pub const fn segmented_piece(&self) -> Option<u16> {
        self.segmented_piece
    }

    /// Whether reads reach the wire as several transactions.
    pub const fn splits_reads(&self) -> bool {
        self.splits_reads
    }

    /// The stretch timeout in microseconds (`None`: unknown).
    pub const fn stretch_timeout_us(&self) -> Option<u32> {
        self.stretch_timeout_us
    }
}

#[cfg(test)]
mod tests;
