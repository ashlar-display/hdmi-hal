//! Simulated backend example for `hdmi-hal`.
//!
//! Demonstrates the implementation pattern for [`ScdcTransport`] and [`HdmiPhy`]:
//! a concrete backend backed by an in-memory register array, and consumer functions
//! generic over the trait — the signature pattern all downstream crates will follow.
//!
//! Run with `cargo run` from this directory.

use core::convert::Infallible;
use display_types::cea861::hdmi_forum::HdmiForumFrl;
use hdmi_hal::phy::{EqParams, FrlOutput, HdmiPhy, LanePatterns, LtpPattern, TxFfeLevel};
use hdmi_hal::scdc::ScdcTransport;

// --- Simulated backends ----------------------------------------------------------

struct SimulatedScdc {
    registers: [u8; 256],
}

impl SimulatedScdc {
    fn new() -> Self {
        Self {
            registers: [0u8; 256],
        }
    }
}

impl ScdcTransport for SimulatedScdc {
    type Error = Infallible;

    fn read(&self, reg: u8) -> Result<u8, Infallible> {
        // `reg` is u8 (0–255) and `registers` has exactly 256 entries, so this
        // index is always in bounds — no panic is possible.
        Ok(self.registers[reg as usize])
    }

    fn write(&mut self, reg: u8, value: u8) -> Result<(), Infallible> {
        // Same guarantee as `read`: u8 address space matches array length exactly.
        self.registers[reg as usize] = value;
        Ok(())
    }

    fn read_block(&self, reg: u8, buf: &mut [u8]) -> Result<(), Infallible> {
        // A register array holds still while it is read, so one copy is one
        // transaction. Past 0xFF it wraps rather than panicking.
        for (offset, byte) in buf.iter_mut().enumerate() {
            *byte = self.registers[(reg as usize + offset) % 256];
        }
        Ok(())
    }
}

struct SimulatedPhy;

impl HdmiPhy for SimulatedPhy {
    type Error = Infallible;

    fn set_frl_rate(&mut self, rate: HdmiForumFrl) -> Result<(), Infallible> {
        println!("PHY: set_frl_rate({rate:?})");
        Ok(())
    }

    fn send_ltp(&mut self, patterns: LanePatterns) -> Result<(), Infallible> {
        println!("PHY: send_ltp({patterns:?})");
        Ok(())
    }

    fn set_frl_output(&mut self, output: FrlOutput) -> Result<(), Infallible> {
        println!("PHY: set_frl_output({output:?})");
        Ok(())
    }

    fn adjust_equalization(&mut self, params: EqParams) -> Result<(), Infallible> {
        println!("PHY: adjust_equalization({params:?})");
        Ok(())
    }

    fn set_scrambling(&mut self, enabled: bool) -> Result<(), Infallible> {
        println!("PHY: set_scrambling({enabled})");
        Ok(())
    }
}

// --- Consumer functions ----------------------------------------------------------
//
// These are the signature patterns that downstream crates (scdc, frl-training,
// platform backends) will follow: generic over the trait, not the concrete type.

fn exercise_scdc<T>(transport: &mut T)
where
    T: ScdcTransport,
    T::Error: core::fmt::Debug,
{
    transport.write(0x20, 0x01).unwrap();
    let val = transport.read(0x20).unwrap();
    println!("SCDC: wrote 0x01 to reg 0x20, read back 0x{val:02x}");
    let mut block = [0u8; 2];
    transport.read_block(0x20, &mut block).unwrap();
    println!("SCDC: read_block from reg 0x20: {block:02x?}");
}

fn exercise_phy<P>(phy: &mut P)
where
    P: HdmiPhy,
    P::Error: core::fmt::Debug,
{
    phy.set_frl_rate(HdmiForumFrl::Rate6Gbps4Lanes).unwrap();
    phy.send_ltp(LanePatterns {
        lane0: Some(LtpPattern::Lfsr0),
        lane1: Some(LtpPattern::Lfsr1),
        lane2: Some(LtpPattern::Lfsr2),
        lane3: Some(LtpPattern::Lfsr3),
    })
    .unwrap();
    let mut eq = EqParams::new();
    eq.lane0.tx_ffe_level = TxFfeLevel::new(1).unwrap();
    phy.adjust_equalization(eq).unwrap();
    phy.send_ltp(LanePatterns::default()).unwrap();
    phy.set_frl_output(FrlOutput::GapOnly).unwrap();
    phy.set_scrambling(true).unwrap();
    phy.set_frl_output(FrlOutput::Active).unwrap();
}

// --------------------------------------------------------------------------------

fn main() {
    let mut scdc = SimulatedScdc::new();
    exercise_scdc(&mut scdc);

    let mut phy = SimulatedPhy;
    exercise_phy(&mut phy);
}
