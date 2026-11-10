//! Hardware abstraction traits for the HDMI stack.
//!
//! `hdmi-hal` defines the behavioral contracts between protocol logic and hardware.
//! It holds traits and the glue between its own traits ([`scdc::read_block_bytewise`],
//! [`scdc::ScdcOverDdc`]): no hardware, platform or clock implementations, no policy.

#![no_std]
#![forbid(unsafe_code)]

pub mod cancel;

pub mod ddc;

/// PHY control traits and associated types.
pub mod phy;

/// SCDC register transport trait.
pub mod scdc;

pub mod time;

/// The FRL rate, as `phy::FrlPhy::set_frl_rate` takes it; re-exported from `display-types` so
/// that implementations name the same type hdmi-hal does.
pub use display_types::cea861::hdmi_forum::HdmiForumFrl;

/// The colour depth and format `phy::TmdsVideo` carries; re-exported from `display-types`
/// so that implementations name the same types hdmi-hal does.
pub use display_types::{ColorBitDepth, ColorFormat};
