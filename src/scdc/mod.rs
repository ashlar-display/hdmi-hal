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
pub trait ScdcTransport {
    /// Error type returned by transport operations.
    type Error;

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

#[cfg(test)]
mod tests {
    use super::*;
    use core::cell::Cell;

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
        type Error = u8;

        fn read(&self, reg: u8) -> Result<u8, Self::Error> {
            if self.fail_at == Some(reg) {
                return Err(reg);
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
        assert_eq!(t.read_block(0x40, &mut buf), Err(0x41));
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
        let dyn_t: &dyn ScdcTransport<Error = u8> = &t;
        let mut buf = [0u8; 2];
        read_block_bytewise(dyn_t, 0x50, &mut buf).unwrap();
        assert_eq!(buf, [0x50, 0x51]);
    }
}
