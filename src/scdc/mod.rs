use core::ops::Deref;

use crate::ddc::{DdcBus, DdcError, SCDC};

/// Raw register access to the SCDC register map over DDC/I²C.
///
/// SCDC is a bidirectional channel used for FRL link training initiation, scrambling
/// control, and CED (Character Error Detection) reporting. Any code that needs to touch
/// the SCDC register map does so through this trait.
///
/// The trait operates at the raw register level: a one-byte address and a one-byte
/// value. Typed register wrappers, named constants, and multi-register sequences belong
/// in the `culvert` crate, not here.
///
/// Every method is required. A transport that can read several registers in one DDC/I²C
/// transaction implements [`read_block`](Self::read_block) with that transaction; one
/// that cannot implements it with [`read_block_bytewise`], so the choice is visible in its
/// code. Here, a bus that can read only one register per transaction:
///
/// ```
/// use core::convert::Infallible;
/// use hdmi_hal::scdc::{ScdcTransport, read_block_bytewise};
///
/// /// A bus whose controller reads one register per transaction.
/// struct OneRegisterBus {
///     registers: [u8; 256], // stands in for the sink
/// }
///
/// impl ScdcTransport for OneRegisterBus {
///     type Error = Infallible;
///
///     fn read(&self, reg: u8) -> Result<u8, Infallible> {
///         Ok(self.registers[reg as usize])
///     }
///
///     fn write(&mut self, reg: u8, value: u8) -> Result<(), Infallible> {
///         self.registers[reg as usize] = value;
///         Ok(())
///     }
///
///     /// One register at a time: not one transaction.
///     fn read_block(&self, reg: u8, buf: &mut [u8]) -> Result<(), Infallible> {
///         read_block_bytewise(self, reg, buf)
///     }
/// }
/// ```
///
/// A simulated sink, a register array that holds still while it is read, conforms with a
/// plain copy instead (see the crate's `simulate` example).
///
/// # Errors
///
/// After any method returns an error, an implementation should leave the transport usable
/// for the next call, recovering the bus itself if a transfer failed midway (for example a
/// held data line after a NACK). One that cannot says so in its documentation: what an
/// error leaves behind, and how to recover (for example, by constructing the transport
/// again).
///
/// Every error carries a [`DdcErrorKind`](crate::ddc::DdcErrorKind), through [`DdcError`]:
/// whether the sink did not answer, the bus timed out or was busy, or the transport is
/// gone. [`DdcError`] is implemented for [`Infallible`](core::convert::Infallible) and `()`.
///
/// Over a [`DdcBus`], [`ScdcOverDdc`] is the transport.
pub trait ScdcTransport {
    /// Error type returned by transport operations, classified by [`DdcError::kind`].
    type Error: DdcError;

    /// Read a single byte from the given SCDC register address.
    fn read(&self, reg: u8) -> Result<u8, Self::Error>;

    /// Write a single byte to the given SCDC register address.
    fn write(&mut self, reg: u8, value: u8) -> Result<(), Self::Error>;

    /// Read consecutive SCDC registers, starting at `reg`, into `buf`.
    ///
    /// Related registers, such as the status flags or the character error counters, are
    /// read together, in one transaction, so that the transport does not split them;
    /// whether the sink keeps them consistent during that transaction is the sink's
    /// (see "No snapshot promised"). The contract:
    ///
    /// - **What it reads.** `buf[i]` is register `reg + i`.
    /// - **One transaction.** The whole buffer is read as one combined transfer to the
    ///   layer below — on the DDC bus, the register offset written, a repeated START,
    ///   `buf.len()` bytes read, STOP — or an error is returned. An implementation never
    ///   splits the read into several transfers, and never falls back to one register at
    ///   a time. Where the layer below is an OS bus driver, the transfer is handed to it
    ///   whole; a driver known to split it on the wire is named in the transport's
    ///   documentation, as the bytewise exception is. Calls made at the same time through
    ///   shared references do not interleave on the bus. [`read_block_bytewise`] is the
    ///   one exception to both, and a transport that uses it says so.
    /// - **No snapshot promised.** One transaction does not mean the sink returns a
    ///   consistent snapshot: whether it holds its registers still during a read is up to
    ///   the sink.
    /// - **Length limit.** An implementation may have a maximum length, if it documents
    ///   it, of at least 16 bytes; a longer buffer is an error, not split. A caller that
    ///   needs a longer read splits it itself — parts of up to 16 bytes fit every
    ///   conforming transport — knowing the parts are separate transactions.
    /// - **Empty buffer.** `Ok`, with no bus activity.
    /// - **On error,** the contents of `buf` are unspecified (see the trait's "Errors" for
    ///   the transport itself).
    /// - **Past `0xFF`.** The caller does not read past register `0xFF`. If it does, the
    ///   result is unspecified, but the call returns, `Ok` or an error, and never panics:
    ///   [`read_block_bytewise`] wraps to `0x00`; a real burst read leaves it to the sink.
    fn read_block(&self, reg: u8, buf: &mut [u8]) -> Result<(), Self::Error>;
}

/// Implements [`ScdcTransport::read_block`] with one [`read`](ScdcTransport::read) per
/// byte, for a transport that cannot read several registers in one transaction.
///
/// This is the documented exception to `read_block`'s "one transaction" rule: each byte
/// is its own transaction, so a value the sink updates between two of them can be read
/// half before and half after the update, and another caller's transaction can land in
/// between. A transport that uses it should say so in its documentation.
///
/// Reads stop at the first error, which is returned. An empty buffer reads nothing. A
/// read past register `0xFF` wraps to `0x00`.
pub fn read_block_bytewise<T: ScdcTransport + ?Sized>(
    transport: &T,
    reg: u8,
    buf: &mut [u8],
) -> Result<(), T::Error> {
    let mut addr = reg;
    for byte in buf {
        *byte = transport.read(addr)?;
        addr = addr.wrapping_add(1);
    }
    Ok(())
}

/// [`ScdcTransport`] over a [`DdcBus`]: the sink's SCDC at [`SCDC`] (0x54).
///
/// - [`read`](ScdcTransport::read) and [`read_block`](ScdcTransport::read_block) are one
///   [`write_read`](DdcBus::write_read) each: the register offset written, then the
///   registers read, in one transaction. `read_block`'s length limit is the bus's read
///   limit for `write_read` ([`DdcCapabilities::write_read_max`](crate::ddc::DdcCapabilities::write_read_max)):
///   the bus refuses a longer read as [`Unsupported`](crate::ddc::DdcErrorKind::Unsupported).
/// - [`write`](ScdcTransport::write) is one [`write`](DdcBus::write) of the offset and the
///   value.
/// - Constructed with [`bytewise`](Self::bytewise), `read_block` reads one register per
///   transaction instead ([`read_block_bytewise`]): the documented exception, for a bus
///   that cannot read 16 registers in one transaction.
///
/// It holds a handle to the bus, anything that dereferences to it (`&B`, `Rc<B>`,
/// `Arc<B>`), so other users of the bus (an EDID reader, an HDCP implementation) share it.
/// Its errors are the bus's.
///
/// ```
/// use hdmi_hal::ddc::{DdcBus, DdcCapabilities, EddcTarget, SCDC};
/// use hdmi_hal::scdc::{ScdcOverDdc, ScdcTransport};
/// use core::cell::Cell;
///
/// /// A bus that reaches only SCDC, over a register file.
/// struct Bus {
///     registers: [Cell<u8>; 256],
/// }
///
/// impl DdcBus for Bus {
///     type Error = ();
///
///     fn capabilities(&self) -> DdcCapabilities {
///         DdcCapabilities::none()
///             .with_address_reachable(SCDC, true)
///             .with_write_max(2)
///             .with_write_read_max(1, 32)
///     }
///
///     fn write(&self, _addr: u8, bytes: &[u8]) -> Result<(), ()> {
///         self.registers[usize::from(bytes[0])].set(bytes[1]);
///         Ok(())
///     }
///
///     fn write_read(&self, _addr: u8, bytes: &[u8], buf: &mut [u8]) -> Result<(), ()> {
///         for (i, b) in buf.iter_mut().enumerate() {
///             *b = self.registers[usize::from(bytes[0]) + i].get();
///         }
///         Ok(())
///     }
///
///     fn read(&self, _addr: u8, _buf: &mut [u8]) -> Result<(), ()> {
///         Err(())
///     }
///
///     fn read_segmented(&self, _: EddcTarget, _: u8, _: u8, _: &mut [u8]) -> Result<(), ()> {
///         Err(())
///     }
/// }
///
/// let bus = Bus { registers: core::array::from_fn(|_| Cell::new(0)) };
/// let mut scdc = ScdcOverDdc::new(&bus).ok().expect("the bus serves SCDC");
/// scdc.write(0x20, 0x03).unwrap();
/// assert_eq!(scdc.read(0x20), Ok(0x03));
/// ```
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ScdcOverDdc<H> {
    bus: H,
    bytewise: bool,
}

impl<H> ScdcOverDdc<H>
where
    H: Deref,
    H::Target: DdcBus,
{
    /// SCDC over `bus`, with `read_block` in one transaction.
    ///
    /// Returns the handle back when the bus cannot serve [`ScdcTransport`]: when it does not
    /// reach [`SCDC`], cannot write an offset and a value, or cannot read 16 registers after
    /// a one-byte offset in one `write_read` (`read_block`'s minimum).
    pub fn new(bus: H) -> Result<Self, H> {
        Self::checked(bus, 16, false)
    }

    /// SCDC over `bus`, with `read_block` one register per transaction
    /// ([`read_block_bytewise`]), for a bus that cannot read 16 registers at once.
    ///
    /// Returns the handle back when the bus does not reach [`SCDC`], cannot write an offset
    /// and a value, or cannot read one register after a one-byte offset.
    pub fn bytewise(bus: H) -> Result<Self, H> {
        Self::checked(bus, 1, true)
    }

    fn checked(bus: H, min_read: u16, bytewise: bool) -> Result<Self, H> {
        let caps = bus.capabilities();
        let (write_part, read_part) = caps.write_read_max();
        if caps.reaches(SCDC) && caps.write_max() >= 2 && write_part >= 1 && read_part >= min_read {
            Ok(ScdcOverDdc { bus, bytewise })
        } else {
            Err(bus)
        }
    }

    /// Whether `read_block` reads one register per transaction.
    pub fn is_bytewise(&self) -> bool {
        self.bytewise
    }

    /// The bus handle.
    pub fn into_inner(self) -> H {
        self.bus
    }
}

impl<H> ScdcTransport for ScdcOverDdc<H>
where
    H: Deref,
    H::Target: DdcBus,
{
    type Error = <H::Target as DdcBus>::Error;

    fn read(&self, reg: u8) -> Result<u8, Self::Error> {
        let mut value = [0u8];
        self.bus.write_read(SCDC, &[reg], &mut value)?;
        Ok(value[0])
    }

    fn write(&mut self, reg: u8, value: u8) -> Result<(), Self::Error> {
        self.bus.write(SCDC, &[reg, value])
    }

    fn read_block(&self, reg: u8, buf: &mut [u8]) -> Result<(), Self::Error> {
        if self.bytewise {
            read_block_bytewise(self, reg, buf)
        } else {
            self.bus.write_read(SCDC, &[reg], buf)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use core::cell::Cell;

    /// A failed read, carrying its register.
    #[derive(Debug, PartialEq)]
    struct Fail(u8);

    impl DdcError for Fail {
        fn kind(&self) -> crate::ddc::DdcErrorKind {
            crate::ddc::DdcErrorKind::Other
        }
    }

    struct MockTransport {
        registers: [u8; 256],
        reads: Cell<u32>,
        fail_at: Option<u8>,
    }

    impl MockTransport {
        fn new() -> Self {
            let mut registers = [0u8; 256];
            for (i, r) in registers.iter_mut().enumerate() {
                *r = i as u8;
            }
            Self {
                registers,
                reads: Cell::new(0),
                fail_at: None,
            }
        }
    }

    impl ScdcTransport for MockTransport {
        type Error = Fail;

        fn read(&self, reg: u8) -> Result<u8, Self::Error> {
            if self.fail_at == Some(reg) {
                return Err(Fail(reg));
            }
            self.reads.set(self.reads.get() + 1);
            Ok(self.registers[reg as usize])
        }

        fn write(&mut self, reg: u8, value: u8) -> Result<(), Self::Error> {
            self.registers[reg as usize] = value;
            Ok(())
        }

        fn read_block(&self, reg: u8, buf: &mut [u8]) -> Result<(), Self::Error> {
            read_block_bytewise(self, reg, buf)
        }
    }

    #[test]
    fn bytewise_reads_consecutive_registers() {
        let t = MockTransport::new();
        let mut buf = [0u8; 3];
        t.read_block(0x40, &mut buf).unwrap();
        assert_eq!(buf, [0x40, 0x41, 0x42]);
        assert_eq!(t.reads.get(), 3);
    }

    #[test]
    fn bytewise_empty_buffer_reads_nothing() {
        let t = MockTransport::new();
        t.read_block(0x40, &mut []).unwrap();
        assert_eq!(t.reads.get(), 0);
    }

    #[test]
    fn bytewise_stops_at_first_error() {
        let mut t = MockTransport::new();
        t.fail_at = Some(0x41);
        let mut buf = [0u8; 3];
        assert_eq!(t.read_block(0x40, &mut buf), Err(Fail(0x41)));
        assert_eq!(buf, [0x40, 0x00, 0x00]);
        assert_eq!(t.reads.get(), 1);
    }

    #[test]
    fn bytewise_wraps_past_0xff() {
        let t = MockTransport::new();
        let mut buf = [0u8; 2];
        t.read_block(0xFF, &mut buf).unwrap();
        assert_eq!(buf, [0xFF, 0x00]);
    }

    #[test]
    fn write_then_read_block() {
        let mut t = MockTransport::new();
        t.write(0x20, 0xAB).unwrap();
        let mut buf = [0u8; 1];
        t.read_block(0x20, &mut buf).unwrap();
        assert_eq!(buf, [0xAB]);
    }

    #[test]
    fn bytewise_through_a_trait_object() {
        // `?Sized`: the helper also serves a `dyn ScdcTransport`.
        let t = MockTransport::new();
        let dyn_t: &dyn ScdcTransport<Error = Fail> = &t;
        let mut buf = [0u8; 2];
        read_block_bytewise(dyn_t, 0x50, &mut buf).unwrap();
        assert_eq!(buf, [0x50, 0x51]);
    }

    use crate::ddc::{DdcCapabilities, DdcErrorKind, EddcTarget};

    /// A DDC bus over an SCDC register file, recording the transactions it sees.
    struct Bus {
        caps: DdcCapabilities,
        registers: [Cell<u8>; 256],
        writes: Cell<u32>,
        write_reads: Cell<u32>,
        last_addr: Cell<u8>,
        fail: Option<DdcErrorKind>,
    }

    impl Bus {
        fn with(caps: DdcCapabilities) -> Self {
            Bus {
                caps,
                registers: core::array::from_fn(|i| Cell::new(i as u8)),
                writes: Cell::new(0),
                write_reads: Cell::new(0),
                last_addr: Cell::new(0),
                fail: None,
            }
        }

        fn scdc(read_limit: u16) -> Self {
            Self::with(
                DdcCapabilities::none()
                    .with_address_reachable(SCDC, true)
                    .with_write_max(2)
                    .with_write_read_max(1, read_limit),
            )
        }
    }

    impl DdcBus for Bus {
        type Error = DdcErrorKind;

        fn capabilities(&self) -> DdcCapabilities {
            self.caps
        }

        fn write(&self, addr: u8, bytes: &[u8]) -> Result<(), DdcErrorKind> {
            if let Some(kind) = self.fail {
                return Err(kind);
            }
            self.last_addr.set(addr);
            self.writes.set(self.writes.get() + 1);
            self.registers[usize::from(bytes[0])].set(bytes[1]);
            Ok(())
        }

        fn write_read(&self, addr: u8, bytes: &[u8], buf: &mut [u8]) -> Result<(), DdcErrorKind> {
            if buf.is_empty() {
                return Ok(());
            }
            if let Some(kind) = self.fail {
                return Err(kind);
            }
            if buf.len() > usize::from(self.caps.write_read_max().1) {
                return Err(DdcErrorKind::Unsupported);
            }
            self.last_addr.set(addr);
            self.write_reads.set(self.write_reads.get() + 1);
            for (i, b) in buf.iter_mut().enumerate() {
                *b = self.registers[usize::from(bytes[0].wrapping_add(i as u8))].get();
            }
            Ok(())
        }

        fn read(&self, _addr: u8, _buf: &mut [u8]) -> Result<(), DdcErrorKind> {
            Err(DdcErrorKind::Unsupported)
        }

        fn read_segmented(
            &self,
            _target: EddcTarget,
            _segment: u8,
            _offset: u8,
            _buf: &mut [u8],
        ) -> Result<(), DdcErrorKind> {
            Err(DdcErrorKind::Unsupported)
        }
    }

    #[test]
    fn over_ddc_reads_and_writes_at_0x54_in_one_transaction_each() {
        let bus = Bus::scdc(32);
        let mut scdc = ScdcOverDdc::new(&bus).ok().unwrap();
        assert!(!scdc.is_bytewise());

        scdc.write(0x20, 0x03).unwrap();
        assert_eq!((bus.writes.get(), bus.last_addr.get()), (1, SCDC));
        assert_eq!(scdc.read(0x20), Ok(0x03));

        let mut ced = [0u8; 7];
        scdc.read_block(0x50, &mut ced).unwrap();
        assert_eq!(ced, [0x50, 0x51, 0x52, 0x53, 0x54, 0x55, 0x56]);
        assert_eq!(bus.write_reads.get(), 2);
    }

    #[test]
    fn over_ddc_empty_block_read_touches_nothing() {
        let bus = Bus::scdc(32);
        let scdc = ScdcOverDdc::new(&bus).ok().unwrap();
        scdc.read_block(0x50, &mut []).unwrap();
        assert_eq!(bus.write_reads.get(), 0);
    }

    #[test]
    fn over_ddc_leaves_reads_beyond_the_bus_limit_to_the_bus() {
        let bus = Bus::scdc(16);
        let scdc = ScdcOverDdc::new(&bus).ok().unwrap();
        let mut long = [0u8; 17];
        assert_eq!(
            scdc.read_block(0x00, &mut long),
            Err(DdcErrorKind::Unsupported)
        );
    }

    #[test]
    fn over_ddc_passes_bus_errors_through() {
        let mut bus = Bus::scdc(32);
        bus.fail = Some(DdcErrorKind::Timeout);
        let mut scdc = ScdcOverDdc::new(&bus).ok().unwrap();
        assert_eq!(scdc.read(0x21), Err(DdcErrorKind::Timeout));
        assert_eq!(scdc.write(0x30, 0x01), Err(DdcErrorKind::Timeout));
    }

    #[test]
    fn over_ddc_refuses_a_bus_that_cannot_serve_scdc() {
        // Does not reach 0x54.
        let bus = Bus::with(DdcCapabilities::general().with_address_reachable(SCDC, false));
        assert!(ScdcOverDdc::new(&bus).is_err());
        assert!(ScdcOverDdc::bytewise(&bus).is_err());
        // Cannot write an offset and a value.
        let bus = Bus::with(DdcCapabilities::general().with_write_max(1));
        assert!(ScdcOverDdc::new(&bus).is_err());
        // Cannot write the offset of a write_read.
        let bus = Bus::with(DdcCapabilities::general().with_write_read_max(0, 32));
        assert!(ScdcOverDdc::new(&bus).is_err());
        // Reads fewer than 16 registers at once: only bytewise.
        let bus = Bus::scdc(15);
        assert!(ScdcOverDdc::new(&bus).is_err());
        assert!(ScdcOverDdc::bytewise(&bus).is_ok());
        // Reads nothing.
        let bus = Bus::scdc(0);
        assert!(ScdcOverDdc::bytewise(&bus).is_err());
    }

    #[test]
    fn over_ddc_hands_the_handle_back() {
        let bus = Bus::scdc(1);
        let refused = ScdcOverDdc::new(&bus).err().unwrap();
        assert!(core::ptr::eq(refused, &bus));
        let scdc = ScdcOverDdc::bytewise(&bus).ok().unwrap();
        assert!(core::ptr::eq(scdc.into_inner(), &bus));
    }

    #[test]
    fn over_ddc_bytewise_reads_one_register_per_transaction() {
        let bus = Bus::scdc(1);
        let scdc = ScdcOverDdc::bytewise(&bus).ok().unwrap();
        assert!(scdc.is_bytewise());
        let mut buf = [0u8; 3];
        scdc.read_block(0x40, &mut buf).unwrap();
        assert_eq!(buf, [0x40, 0x41, 0x42]);
        assert_eq!(bus.write_reads.get(), 3);
    }

    /// A handle that is neither a reference nor a standard pointer.
    struct Handle<'a>(&'a Bus);

    impl Deref for Handle<'_> {
        type Target = Bus;
        fn deref(&self) -> &Bus {
            self.0
        }
    }

    #[test]
    fn over_ddc_needs_no_plain_or_segmented_reads() {
        // The bus refuses both, so every SCDC access above went through write and write_read.
        let bus = Bus::scdc(32);
        let mut buf = [0u8; 1];
        assert_eq!(bus.read(SCDC, &mut buf), Err(DdcErrorKind::Unsupported));
        assert_eq!(
            bus.read_segmented(EddcTarget::Edid, 0, 0, &mut buf),
            Err(DdcErrorKind::Unsupported)
        );
        assert_eq!(Fail(0).kind(), DdcErrorKind::Other);
    }

    #[test]
    fn over_ddc_takes_any_handle() {
        let bus = Bus::scdc(32);
        let scdc = ScdcOverDdc::new(Handle(&bus)).ok().unwrap();
        assert_eq!(scdc.read(0x07), Ok(0x07));
    }
}
