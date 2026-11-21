use super::*;
use core::cell::Cell;

#[test]
fn usable_addresses_exclude_the_reserved_ranges() {
    assert!(!is_usable_address(0x00));
    assert!(!is_usable_address(0x07));
    assert!(is_usable_address(0x08));
    assert!(is_usable_address(0x77));
    assert!(!is_usable_address(0x78));
    assert!(!is_usable_address(0x7F));
    assert!(!is_usable_address(0x80));
    assert!(!is_usable_address(0xFF));
    for addr in [
        SEGMENT_POINTER,
        HDCP_PRIMARY,
        HDCP_SECONDARY,
        DDC_CI,
        EDID,
        DISPLAYID,
        SCDC,
    ] {
        assert!(is_usable_address(addr), "{addr:#04x}");
    }
}

#[test]
fn general_reaches_every_usable_address_without_limits() {
    let caps = DdcCapabilities::general();
    for addr in 0u8..=0xFF {
        assert_eq!(caps.reaches(addr), is_usable_address(addr), "{addr:#04x}");
    }
    assert_eq!(caps.write_max(), u16::MAX);
    assert_eq!(caps.write_read_max(), (u16::MAX, u16::MAX));
    assert_eq!(caps.read_max(), u16::MAX);
    assert!(caps.reads_target(EddcTarget::Edid));
    assert!(caps.reads_target(EddcTarget::DisplayId));
    assert_eq!(caps.max_segment(), Some(MAX_SEGMENT));
    assert_eq!(caps.segmented_piece(), None);
    assert!(!caps.splits_reads());
    assert_eq!(caps.stretch_timeout_us(), None);
}

#[test]
fn none_reaches_nothing() {
    let caps = DdcCapabilities::none();
    assert_eq!(caps.reachable_addresses(), 0);
    assert_eq!(caps.write_max(), 0);
    assert_eq!(caps.write_read_max(), (0, 0));
    assert_eq!(caps.read_max(), 0);
    assert!(!caps.reads_target(EddcTarget::Edid));
    assert!(!caps.reads_target(EddcTarget::DisplayId));
    assert_eq!(caps.max_segment(), None);
}

#[test]
fn setters_change_one_field_each() {
    let caps = DdcCapabilities::general()
        .with_write_max(33)
        .with_write_read_max(1, 32)
        .with_read_max(1)
        .with_segmented_piece(Some(32))
        .with_splits_reads(true)
        .with_stretch_timeout_us(Some(2_000));
    assert_eq!(caps.write_max(), 33);
    assert_eq!(caps.write_read_max(), (1, 32));
    assert_eq!(caps.read_max(), 1);
    assert_eq!(caps.segmented_piece(), Some(32));
    assert!(caps.splits_reads());
    assert_eq!(caps.stretch_timeout_us(), Some(2_000));
    assert_eq!(
        caps.reachable_addresses(),
        DdcCapabilities::general().reachable_addresses()
    );
}

#[test]
fn address_setters_ignore_unusable_addresses() {
    let caps = DdcCapabilities::none()
        .with_address_reachable(SCDC, true)
        .with_address_reachable(0x00, true)
        .with_address_reachable(0x7F, true)
        .with_address_reachable(0xC0, true);
    assert!(caps.reaches(SCDC));
    assert_eq!(caps.reachable_addresses(), 1u128 << SCDC);

    let caps = DdcCapabilities::none().with_reachable_addresses(u128::MAX);
    assert_eq!(
        caps,
        DdcCapabilities::none().with_reachable_addresses(USABLE_ADDRESSES)
    );
    assert!(!caps.reaches(0x00));
    assert!(caps.reaches(0x08));

    let caps = DdcCapabilities::general().with_address_reachable(DDC_CI, false);
    assert!(!caps.reaches(DDC_CI));
    assert!(caps.reaches(EDID));
}

#[test]
fn targets_toggle_independently() {
    let caps = DdcCapabilities::general().with_eddc_target(EddcTarget::DisplayId, false);
    assert!(caps.reads_target(EddcTarget::Edid));
    assert!(!caps.reads_target(EddcTarget::DisplayId));
    let caps = caps.with_eddc_target(EddcTarget::DisplayId, true);
    assert!(caps.reads_target(EddcTarget::DisplayId));
}

#[test]
fn max_segment_is_capped() {
    assert_eq!(
        DdcCapabilities::none()
            .with_max_segment(Some(0))
            .max_segment(),
        Some(0)
    );
    assert_eq!(
        DdcCapabilities::none()
            .with_max_segment(Some(0xFF))
            .max_segment(),
        Some(MAX_SEGMENT)
    );
    assert_eq!(
        DdcCapabilities::general()
            .with_max_segment(None)
            .max_segment(),
        None
    );
}

#[test]
fn capabilities_build_in_const_context() {
    const SMBUS: DdcCapabilities = DdcCapabilities::general()
        .with_write_max(33)
        .with_write_read_max(1, 32)
        .with_read_max(1)
        .with_max_segment(Some(0));
    assert_eq!(SMBUS.max_segment(), Some(0));
}

#[test]
fn eddc_targets_have_their_addresses() {
    assert_eq!(EddcTarget::Edid.address(), 0x50);
    assert_eq!(EddcTarget::DisplayId.address(), 0x52);
}

#[test]
fn unit_error_is_other() {
    assert_eq!(().kind(), DdcErrorKind::Other);
}

#[test]
fn a_kind_is_its_own_error() {
    let kind = DdcErrorKind::NoAcknowledge(NoAcknowledgeSource::Data);
    assert_eq!(kind.kind(), kind);
}

#[test]
fn infallible_is_a_ddc_error() {
    fn kind_of<E: DdcError>(result: Result<(), E>) -> Option<DdcErrorKind> {
        result.err().map(|e| e.kind())
    }
    assert_eq!(kind_of(Ok::<(), Infallible>(())), None);
}

/// A bus over a 256-byte EDID store and an SCDC register file, with interior mutability:
/// the `&self` contract implemented with a `Cell`, counting transactions.
struct MockBus {
    edid: [u8; 256],
    scdc: [Cell<u8>; 256],
    transactions: Cell<u32>,
}

impl MockBus {
    fn new() -> Self {
        let mut edid = [0u8; 256];
        for (i, b) in edid.iter_mut().enumerate() {
            *b = i as u8;
        }
        MockBus {
            edid,
            scdc: core::array::from_fn(|_| Cell::new(0)),
            transactions: Cell::new(0),
        }
    }
}

impl DdcBus for MockBus {
    type Error = DdcErrorKind;

    fn capabilities(&self) -> DdcCapabilities {
        DdcCapabilities::none()
            .with_address_reachable(SCDC, true)
            .with_write_max(2)
            .with_write_read_max(1, 32)
            .with_eddc_target(EddcTarget::Edid, true)
            .with_max_segment(Some(0))
    }

    fn write(&self, addr: u8, bytes: &[u8]) -> Result<(), DdcErrorKind> {
        if addr != SCDC || bytes.len() != 2 {
            return Err(DdcErrorKind::Unsupported);
        }
        self.transactions.set(self.transactions.get() + 1);
        self.scdc[usize::from(bytes[0])].set(bytes[1]);
        Ok(())
    }

    fn write_read(&self, addr: u8, bytes: &[u8], buf: &mut [u8]) -> Result<(), DdcErrorKind> {
        if buf.is_empty() {
            return Ok(());
        }
        if addr != SCDC || bytes.len() != 1 || buf.len() > 32 {
            return Err(DdcErrorKind::Unsupported);
        }
        self.transactions.set(self.transactions.get() + 1);
        for (i, b) in buf.iter_mut().enumerate() {
            *b = self.scdc[usize::from(bytes[0].wrapping_add(i as u8))].get();
        }
        Ok(())
    }

    fn read(&self, _addr: u8, buf: &mut [u8]) -> Result<(), DdcErrorKind> {
        if buf.is_empty() {
            return Ok(());
        }
        Err(DdcErrorKind::Unsupported)
    }

    fn read_segmented(
        &self,
        target: EddcTarget,
        segment: u8,
        offset: u8,
        buf: &mut [u8],
    ) -> Result<(), DdcErrorKind> {
        let start = usize::from(offset);
        if buf.is_empty() {
            return Ok(());
        }
        if target != EddcTarget::Edid || segment != 0 || start + buf.len() > 256 {
            return Err(DdcErrorKind::Unsupported);
        }
        self.transactions.set(self.transactions.get() + 1);
        buf.copy_from_slice(&self.edid[start..start + buf.len()]);
        Ok(())
    }
}

#[test]
fn a_shared_bus_serves_several_users_through_references() {
    let bus = MockBus::new();
    let scdc_user: &dyn DdcBus<Error = DdcErrorKind> = &bus;
    let edid_user = &bus;

    scdc_user.write(SCDC, &[0x20, 0x03]).unwrap();
    let mut config = [0u8; 1];
    scdc_user.write_read(SCDC, &[0x20], &mut config).unwrap();
    assert_eq!(config, [0x03]);

    let mut block = [0u8; 128];
    edid_user
        .read_segmented(EddcTarget::Edid, 0, 0x80, &mut block)
        .unwrap();
    assert_eq!(block[0], 0x80);
    assert_eq!(bus.transactions.get(), 3);
}

#[test]
fn empty_buffers_cause_no_bus_activity() {
    let bus = MockBus::new();
    bus.write_read(SCDC, &[0x20], &mut []).unwrap();
    bus.read(SCDC, &mut []).unwrap();
    bus.read_segmented(EddcTarget::Edid, 0, 0, &mut []).unwrap();
    assert_eq!(bus.transactions.get(), 0);
}

#[test]
fn what_the_capabilities_rule_out_is_unsupported() {
    let bus = MockBus::new();
    let caps = bus.capabilities();
    assert!(!caps.reaches(DDC_CI));
    assert_eq!(
        bus.write(DDC_CI, &[0x51, 0x81]),
        Err(DdcErrorKind::Unsupported)
    );
    assert!(!caps.reads_target(EddcTarget::DisplayId));
    let mut buf = [0u8; 8];
    assert_eq!(
        bus.read_segmented(EddcTarget::DisplayId, 0, 0, &mut buf)
            .unwrap_err()
            .kind(),
        DdcErrorKind::Unsupported
    );
    assert_eq!(caps.max_segment(), Some(0));
    assert_eq!(
        bus.read_segmented(EddcTarget::Edid, 1, 0, &mut buf),
        Err(DdcErrorKind::Unsupported)
    );
}
