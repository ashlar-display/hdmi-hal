//! Hardware abstraction traits for the HDMI stack.
//!
//! `hdmi-hal` defines the behavioral contracts between protocol logic and hardware.
//! It is a traits-only crate: no implementations live here.

#![no_std]
#![forbid(unsafe_code)]

pub mod ddc;

/// PHY control trait and associated types.
pub mod phy;

/// SCDC register transport trait.
pub mod scdc;

/// The FRL rate, as `HdmiPhy::set_frl_rate` takes it; re-exported from `display-types` so
/// that implementations name the same type hdmi-hal does.
pub use display_types::cea861::hdmi_forum::HdmiForumFrl;
