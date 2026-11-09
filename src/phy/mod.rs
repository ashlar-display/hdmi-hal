use display_types::cea861::hdmi_forum::HdmiForumFrl;

/// A link training pattern to be driven on the physical lanes.
///
/// Produced by the link training state machine and passed to [`FrlPhy::send_ltp`].
/// The discriminants are the pattern values a sink requests in the SCDC Status_Flags
/// LTP fields. Request values that are not patterns (0 for no pattern, and the TxFFE
/// and rate change requests) are handled by the link training layer and have no variant.
#[non_exhaustive]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[repr(u8)]
pub enum LtpPattern {
    /// All ones.
    AllOnes = 1,
    /// All zeros.
    AllZeros = 2,
    /// Nyquist clock pattern.
    NyquistClock = 3,
    /// DDE (Data Dependent Equalization) compliance pattern.
    DdeCompliance = 4,
    /// LFSR pattern 0.
    Lfsr0 = 5,
    /// LFSR pattern 1.
    Lfsr1 = 6,
    /// LFSR pattern 2.
    Lfsr2 = 7,
    /// LFSR pattern 3.
    Lfsr3 = 8,
}

impl LtpPattern {
    /// Returns the pattern's SCDC value (1–8).
    pub const fn value(self) -> u8 {
        self as u8
    }
}

/// The link training pattern for each lane, passed to [`FrlPhy::send_ltp`].
///
/// Always the full per-lane set: the PHY applies it as given, and the link training
/// layer tracks which pattern each lane carries. `None` means no training pattern on
/// that lane; `None` on every lane stops the training patterns. `lane3` is `None` in
/// 3-lane FRL mode.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Hash)]
pub struct LanePatterns {
    /// Pattern for lane 0.
    pub lane0: Option<LtpPattern>,
    /// Pattern for lane 1.
    pub lane1: Option<LtpPattern>,
    /// Pattern for lane 2.
    pub lane2: Option<LtpPattern>,
    /// Pattern for lane 3. `None` in 3-lane FRL mode.
    pub lane3: Option<LtpPattern>,
}

/// What the transmitter sends on the FRL lanes once training patterns are stopped.
///
/// Passed to [`FrlPhy::set_frl_output`].
///
/// Deliberately exhaustive, unlike [`LtpPattern`]. The two variants are the two states of
/// an FRL link, training and carrying content, not a list that grows with each HDMI
/// version. A PHY has to implement both, and has no sensible behaviour for a mode it does
/// not know: a wildcard arm could only fail. If a later HDMI version adds a mode, it should
/// reach every implementation as a compile error in a breaking release, not as a runtime
/// error from a PHY that was never updated.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum FrlOutput {
    /// Gap characters only, with no video, data islands or control periods. Used during
    /// link training and while waiting for the sink to start FRL.
    GapOnly,
    /// Video, data islands and control periods. Set by the caller once link training
    /// has succeeded.
    Active,
}

/// A TxFFE (transmitter feed-forward equalization) level index, 0–7.
///
/// The level a lane's transmitter applies during FRL training. The range is checked on
/// construction; the lower per-rate limit the source advertises to the sink is the link
/// training layer's concern.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct TxFfeLevel(u8);

impl TxFfeLevel {
    /// The highest TxFFE level, 7.
    pub const MAX: Self = Self(7);

    /// Returns the level for `level`, or `None` if it is above 7.
    pub const fn new(level: u8) -> Option<Self> {
        if level <= Self::MAX.0 {
            Some(Self(level))
        } else {
            None
        }
    }

    /// Returns the level index (0–7).
    pub const fn value(self) -> u8 {
        self.0
    }
}

/// Per-lane equalization parameters carried by [`EqParams`].
#[non_exhaustive]
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Hash)]
pub struct LaneEqParams {
    /// TxFFE level for the lane. Defaults to 0.
    pub tx_ffe_level: TxFfeLevel,
}

/// Restrictions on the TxFFE components, as a compliance tester sets them.
///
/// TxFFE combines two components: pre-shoot (pre-cursor) and de-emphasis (post-cursor).
/// The sink's `Source_Test_Configuration` register (SCDC 0x35, bits 1–3) can restrict them,
/// and the three bits are carried here exactly as written, for the whole link. Each bit's
/// plain meaning:
///
/// - `pre_shoot_only` (`TxFFE_Pre_Shoot_Only`): pre-shoot only, no de-emphasis.
/// - `de_emphasis_only` (`TxFFE_De_Emphasis_Only`): de-emphasis only, no pre-shoot.
/// - `no_ffe` (`TxFFE_No_FFE`): neither.
///
/// The default (no bit set) restricts nothing, for every backend: full TxFFE, as before
/// the restrictions existed. The bits accompany a compliance test. Of the reference
/// drivers that act on them, the Rockchip one does so only after the sink requests the DDE
/// compliance pattern (LTP 4) on a lane; the AMD one applies them with every TxFFE setting
/// during link training.
///
/// How to combine the bits is the PHY's to decide, because the reference implementations
/// disagree and no public source settles it. The AMD display
/// driver (`dpcs30_program_eq_setting`) treats each "only" bit as removing the other
/// component, so both set leaves neither (not quite its `no_ffe` setting, which also
/// changes the main cursor), and `no_ffe` overrides both. The Rockchip HDPTX PHY driver
/// treats bits 1–2 as enabling their component, so both set gives full FFE, and does not
/// read `no_ffe`; that reading applies only once a bit is set, never to the default. A
/// backend documents which it follows. One that cannot restrict its TxFFE may ignore the
/// bits, and says so: a compliance test then fails, not the link. The level in each lane's
/// [`LaneEqParams`] still applies within what the restrictions allow.
#[non_exhaustive]
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Hash)]
pub struct TxFfeRestrictions {
    /// `TxFFE_Pre_Shoot_Only` (bit 1): use pre-shoot only.
    pub pre_shoot_only: bool,
    /// `TxFFE_De_Emphasis_Only` (bit 2): use de-emphasis only.
    pub de_emphasis_only: bool,
    /// `TxFFE_No_FFE` (bit 3): use no TxFFE.
    pub no_ffe: bool,
}

impl TxFfeRestrictions {
    /// Constructs `TxFfeRestrictions` from the three bits, in register order.
    pub const fn new(pre_shoot_only: bool, de_emphasis_only: bool, no_ffe: bool) -> Self {
        Self {
            pre_shoot_only,
            de_emphasis_only,
            no_ffe,
        }
    }
}

/// Equalization parameters passed from link training to the PHY.
///
/// Carries each lane's TxFFE level, raised during FRL link training when the sink requests
/// it (an FFE change, 0xE, in its link training requests), and the link-wide
/// [`TxFfeRestrictions`] a compliance tester may set. `lane3` is `None` in 3-lane FRL mode.
#[non_exhaustive]
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Hash)]
pub struct EqParams {
    /// Equalization parameters for lane 0.
    pub lane0: LaneEqParams,
    /// Equalization parameters for lane 1.
    pub lane1: LaneEqParams,
    /// Equalization parameters for lane 2.
    pub lane2: LaneEqParams,
    /// Equalization parameters for lane 3. `None` in 3-lane FRL mode.
    pub lane3: Option<LaneEqParams>,
    /// Restrictions on the TxFFE components, for every lane. Defaults to none.
    pub tx_ffe_restrictions: TxFfeRestrictions,
}

impl EqParams {
    /// Create a new `EqParams` with default values.
    pub fn new() -> Self {
        Self::default()
    }
}

/// FRL control of an HDMI 2.1 transmitter PHY.
///
/// Covers what FRL link training needs from the transmitter: FRL rate selection, link
/// training patterns, FRL output mode, and each lane's TxFFE level (FRL's transmitter
/// equalization, a combination of pre-shoot and de-emphasis) through
/// [`adjust_equalization`](Self::adjust_equalization). TMDS is [`TmdsPhy`]'s, the source's
/// scrambler included, and power is [`PhyPower`]'s. Further PHY configuration (lane mapping,
/// spread-spectrum clocking and power states beyond off and on) is not covered yet; it is
/// planned for when the mode-setting layer needs it.
/// Vendor-specific register sequences are an implementation detail of each backend.
///
/// On a PHY that also implements [`TmdsPhy`], entering one mode leaves the other:
/// [`set_frl_rate`](Self::set_frl_rate) with a rate leaves TMDS.
pub trait FrlPhy {
    /// Error type returned by PHY operations.
    type Error;

    /// Select the FRL rate, or turn FRL off. Triggers the required lane reconfiguration
    /// sequence.
    ///
    /// A rate enters FRL, leaving TMDS on a PHY that also does TMDS.
    /// [`HdmiForumFrl::NotSupported`] turns FRL off and enters no other mode: entering TMDS
    /// is [`TmdsPhy::configure`]'s.
    ///
    /// Returns once the PHY transmits at `rate`, after any bring-up its hardware needs to
    /// get there — the Xilinx HDMI 2.1 transmitter, for example, holds a Nyquist clock
    /// pattern until its link is up. Only the PHY knows when that is, so the sequence
    /// belongs here; the link training layer sends no pattern of its own around it.
    fn set_frl_rate(&mut self, rate: HdmiForumFrl) -> Result<(), Self::Error>;

    /// Drive the given link training patterns on the physical lanes, one per lane.
    fn send_ltp(&mut self, patterns: LanePatterns) -> Result<(), Self::Error>;

    /// Select what the transmitter sends on the FRL lanes: gap characters only, or video,
    /// data islands and control periods.
    fn set_frl_output(&mut self, output: FrlOutput) -> Result<(), Self::Error>;

    /// Apply equalization: each lane's TxFFE level, within the link-wide
    /// [`TxFfeRestrictions`] in `params`. Each call carries the complete settings, so the
    /// restrictions in force are always the last ones given.
    fn adjust_equalization(&mut self, params: EqParams) -> Result<(), Self::Error>;
}

/// The TMDS bit clock ratio, as the sink's `TMDS_Bit_Clock_Ratio` bit selects it.
///
/// Deliberately exhaustive: it is one bit on the wire (`TMDS_Bit_Clock_Ratio`), with no room
/// for a third value.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum TmdsClockRatio {
    /// 1/10.
    Div10,
    /// 1/40.
    Div40,
}

/// The video a TMDS link carries, as far as a PHY may need it.
///
/// Some PHYs are set up from the pixel clock and colour depth rather than from the character
/// rate, and one character rate comes from several pixel clocks (8, 10 and 12 bits per
/// component, 4:2:0). A PHY keyed by the character rate ignores this.
#[non_exhaustive]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TmdsVideo {
    /// The clock at which pixels reach the TMDS encoder, in kHz: pixel repetition included,
    /// the deep-colour factor not. For a repeated mode it is the mode's own clock times
    /// `pixel_repetition + 1`.
    ///
    /// Linux lists 720(1440)x480i (VIC 6) at 13 500 kHz with a double-clock flag
    /// (torvalds/linux `e774d5f1b`, `drivers/gpu/drm/drm_edid.c:791–794`); this field is
    /// 27 000 for it, with `pixel_repetition` 1. Drivers read such a mode either way: i915
    /// doubles the clock for the port (`i915/display/intel_hdmi.c:2049–2052`, `2175–2176`),
    /// while dw-hdmi takes the mode's clock as its pixel clock (`bridge/synopsys/dw-hdmi.c:2004`)
    /// and refuses double-clocked modes (`:2940–2941`). The convention here is chosen, so
    /// that a PHY keyed by the pixel clock needs no other field to find its rate.
    pub pixel_clock_khz: u32,
    /// Bits per colour component.
    pub color_depth: display_types::ColorBitDepth,
    /// The colour format. YCbCr 4:2:0 halves the character rate.
    pub color_format: display_types::ColorFormat,
    /// Pixel repetition: 0 for none, `n` for each pixel sent `n + 1` times.
    pub pixel_repetition: u8,
}

impl TmdsVideo {
    /// Constructs `TmdsVideo` with no pixel repetition.
    pub const fn new(
        pixel_clock_khz: u32,
        color_depth: display_types::ColorBitDepth,
        color_format: display_types::ColorFormat,
    ) -> Self {
        Self {
            pixel_clock_khz,
            color_depth,
            color_format,
            pixel_repetition: 0,
        }
    }

    /// Sets the pixel repetition (0: none; `n`: each pixel sent `n + 1` times).
    pub const fn with_pixel_repetition(mut self, pixel_repetition: u8) -> Self {
        self.pixel_repetition = pixel_repetition;
        self
    }
}

/// What [`TmdsPhy::configure`] sets.
///
/// The caller guarantees that [`character_rate_khz`](Self::character_rate_khz) is the rate
/// [`video`](Self::video) gives, rounded up. A PHY may configure itself from either: the
/// character rate, or the video it comes from.
#[non_exhaustive]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TmdsPhyConfig {
    /// The TMDS character rate, in kHz, rounded up and never truncated.
    pub character_rate_khz: u32,
    /// The TMDS bit clock ratio.
    pub clock_ratio: TmdsClockRatio,
    /// The video the link carries.
    pub video: TmdsVideo,
}

impl TmdsPhyConfig {
    /// Constructs `TmdsPhyConfig`.
    pub const fn new(
        character_rate_khz: u32,
        clock_ratio: TmdsClockRatio,
        video: TmdsVideo,
    ) -> Self {
        Self {
            character_rate_khz,
            clock_ratio,
            video,
        }
    }
}

/// What a TMDS PHY can do, as [`TmdsPhy::capabilities`] reports it.
///
/// Hardware that cannot do something reports it here, never more than it can: [`new`]
/// starts with no scrambler.
///
/// [`new`]: Self::new
#[non_exhaustive]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct TmdsPhyCapabilities {
    /// The source's own highest TMDS character rate, in kHz.
    pub max_character_rate_khz: u32,
    /// Whether the PHY has a TMDS scrambler.
    ///
    /// A field of its own, not derived from the ceiling: a link at exactly 340 000 kHz may
    /// be scrambled, so a PHY whose ceiling is 340 Mcsc and which has no scrambler says so
    /// here.
    pub scrambling: bool,
    /// Whether the PHY can scramble at or below 340 Mcsc. Counts only with
    /// [`scrambling`](Self::scrambling).
    pub low_rate_scrambling: bool,
}

impl TmdsPhyCapabilities {
    /// Constructs `TmdsPhyCapabilities` with the given ceiling and no scrambler.
    pub const fn new(max_character_rate_khz: u32) -> Self {
        Self {
            max_character_rate_khz,
            scrambling: false,
            low_rate_scrambling: false,
        }
    }

    /// Sets whether the PHY has a TMDS scrambler.
    pub const fn with_scrambling(mut self, scrambling: bool) -> Self {
        self.scrambling = scrambling;
        self
    }

    /// Sets whether the PHY can scramble at or below 340 Mcsc.
    pub const fn with_low_rate_scrambling(mut self, low_rate_scrambling: bool) -> Self {
        self.low_rate_scrambling = low_rate_scrambling;
        self
    }
}

/// TMDS control of an HDMI transmitter PHY.
///
/// What TMDS bring-up and tear-down need from the transmitter, and nothing more: the
/// character rate and clock ratio, the source's scrambler, and starting and stopping the
/// output. A trait of its own, so TMDS-only hardware implements nothing of FRL.
///
/// Not covered: electrical settings (swing, pre-emphasis, lane mapping), which are the
/// implementation's; the video pipeline beyond what [`TmdsVideo`] gives the PHY; hotplug and
/// RX sense; and power, which is [`PhyPower`]'s.
///
/// On a PHY that also implements [`FrlPhy`], entering one mode leaves the other:
/// [`configure`](Self::configure) leaves FRL.
pub trait TmdsPhy {
    /// Error type returned by PHY operations.
    type Error: core::fmt::Debug;

    /// What the PHY can do. A plain report of what the implementation already knows.
    fn capabilities(&self) -> TmdsPhyCapabilities;

    /// Set the character rate and clock ratio, with the output stopped, and leave it
    /// stopped. Enters TMDS mode, leaving FRL on a PHY that also does FRL.
    ///
    /// An implementation that cannot reconfigure while transmitting may return an error
    /// when called with the output running.
    fn configure(&mut self, config: TmdsPhyConfig) -> Result<(), Self::Error>;

    /// Turn the source's TMDS scrambler on or off, with the output running or stopped.
    ///
    /// `set_scrambling(false)` succeeds on a PHY without a scrambler
    /// ([`TmdsPhyCapabilities::scrambling`] false), since plain links call it.
    fn set_scrambling(&mut self, enabled: bool) -> Result<(), Self::Error>;

    /// Start TMDS output at the configured rate.
    ///
    /// Returns once the PHY transmits, after whatever its hardware needs to get there (a PLL
    /// lock, a reset sequence): callers time from its return, so returning means on the
    /// wire.
    fn start_output(&mut self) -> Result<(), Self::Error>;

    /// Stop whatever the PHY transmits: TMDS clock and data or, on a PHY still in FRL, FRL
    /// transmission. Keeps the PHY powered and configured, unlike
    /// [`PhyPower::power_off`].
    ///
    /// Callable in any state, powered off included; stopping a stopped output is not an
    /// error.
    fn stop_output(&mut self) -> Result<(), Self::Error>;
}

/// Power control of a transmitter PHY, shared by [`TmdsPhy`] and [`FrlPhy`].
///
/// A trait of its own, not a supertrait of the mode traits: link training and TMDS
/// bring-up bound on a mode trait alone and never change the power, and a PHY that is
/// always on implements the mode traits only. A caller that powers the PHY, on unplug and
/// before a bring-up, bounds on `PhyPower` as well. One PHY implementing several of these
/// traits may use one error type for all of them.
///
/// For a PHY implementing both mode traits, entering one mode leaves the other:
/// [`TmdsPhy::configure`] leaves FRL, and [`FrlPhy::set_frl_rate`] with a rate leaves TMDS;
/// `set_frl_rate(NotSupported)` turns FRL off and enters nothing.
pub trait PhyPower {
    /// Error type returned by power operations.
    type Error: core::fmt::Debug;

    /// Power the PHY on, leaving the output stopped and no mode entered; the caller then
    /// enters one, with [`TmdsPhy::configure`] or [`FrlPhy::set_frl_rate`].
    ///
    /// Powering on a PHY that is on is not an error, and does not change a mode already
    /// entered.
    fn power_on(&mut self) -> Result<(), Self::Error>;

    /// Power the PHY off: stop any output and leave no mode. Afterwards the PHY holds no
    /// TMDS configuration and no FRL rate, so a bring-up must configure it again.
    ///
    /// Callable in any state; powering off a PHY that is off is not an error.
    fn power_off(&mut self) -> Result<(), Self::Error>;
}

#[cfg(test)]
mod tests {
    use super::*;
    use display_types::cea861::hdmi_forum::HdmiForumFrl;

    /// A minimal `FrlPhy` that records what it is given. The tests that drive it method by
    /// method check little beyond recording — they show that the trait can be implemented
    /// and used for each method, and they keep this implementation covered. Behaviour is
    /// tested where it lives: in plumbob, against its simulated PHY.
    struct MockPhy {
        frl_rate: Option<HdmiForumFrl>,
        eq_calls: u32,
        last_eq: Option<EqParams>,
        frl_output: Option<FrlOutput>,
        last_ltp: Option<LanePatterns>,
    }

    impl MockPhy {
        fn new() -> Self {
            Self {
                frl_rate: None,
                eq_calls: 0,
                last_eq: None,
                frl_output: None,
                last_ltp: None,
            }
        }
    }

    impl FrlPhy for MockPhy {
        type Error = core::convert::Infallible;

        fn send_ltp(&mut self, patterns: LanePatterns) -> Result<(), Self::Error> {
            self.last_ltp = Some(patterns);
            Ok(())
        }

        fn set_frl_output(&mut self, output: FrlOutput) -> Result<(), Self::Error> {
            self.frl_output = Some(output);
            Ok(())
        }

        fn set_frl_rate(&mut self, rate: HdmiForumFrl) -> Result<(), Self::Error> {
            self.frl_rate = Some(rate);
            Ok(())
        }

        fn adjust_equalization(&mut self, params: EqParams) -> Result<(), Self::Error> {
            self.eq_calls += 1;
            self.last_eq = Some(params);
            Ok(())
        }
    }

    #[test]
    fn ltp_pattern_values_match_scdc_encoding() {
        assert_eq!(LtpPattern::AllOnes.value(), 1);
        assert_eq!(LtpPattern::AllZeros.value(), 2);
        assert_eq!(LtpPattern::NyquistClock.value(), 3);
        assert_eq!(LtpPattern::DdeCompliance.value(), 4);
        assert_eq!(LtpPattern::Lfsr0.value(), 5);
        assert_eq!(LtpPattern::Lfsr1.value(), 6);
        assert_eq!(LtpPattern::Lfsr2.value(), 7);
        assert_eq!(LtpPattern::Lfsr3.value(), 8);
    }

    #[test]
    fn ltp_pattern_clone_eq() {
        let a = LtpPattern::Lfsr1;
        assert_eq!(a, a);
        assert_ne!(LtpPattern::Lfsr0, LtpPattern::Lfsr1);
    }

    #[test]
    fn lane_patterns_default_is_no_pattern() {
        let p = LanePatterns::default();
        assert_eq!(p.lane0, None);
        assert_eq!(p.lane1, None);
        assert_eq!(p.lane2, None);
        assert_eq!(p.lane3, None);
    }

    #[test]
    fn send_ltp_records_patterns() {
        let mut phy = MockPhy::new();
        let patterns = LanePatterns {
            lane0: Some(LtpPattern::Lfsr0),
            lane1: Some(LtpPattern::Lfsr1),
            lane2: Some(LtpPattern::Lfsr2),
            lane3: None,
        };
        phy.send_ltp(patterns).unwrap();
        assert_eq!(phy.last_ltp, Some(patterns));
    }

    #[test]
    fn send_ltp_updates_on_each_call() {
        let mut phy = MockPhy::new();
        let first = LanePatterns {
            lane0: Some(LtpPattern::Lfsr0),
            ..LanePatterns::default()
        };
        let second = LanePatterns {
            lane0: Some(LtpPattern::NyquistClock),
            ..first
        };
        phy.send_ltp(first).unwrap();
        phy.send_ltp(second).unwrap();
        assert_eq!(phy.last_ltp, Some(second));
    }

    #[test]
    fn eq_params_constructors_are_equivalent() {
        let _a = EqParams::new();
        let _b = EqParams::default();
    }

    #[test]
    fn set_frl_rate_records_rate() {
        let mut phy = MockPhy::new();
        phy.set_frl_rate(HdmiForumFrl::Rate6Gbps4Lanes).unwrap();
        assert_eq!(phy.frl_rate, Some(HdmiForumFrl::Rate6Gbps4Lanes));
    }

    #[test]
    fn set_frl_rate_not_supported_is_valid() {
        let mut phy = MockPhy::new();
        phy.set_frl_rate(HdmiForumFrl::NotSupported).unwrap();
        assert_eq!(phy.frl_rate, Some(HdmiForumFrl::NotSupported));
    }

    #[test]
    fn set_frl_rate_can_be_updated() {
        let mut phy = MockPhy::new();
        phy.set_frl_rate(HdmiForumFrl::Rate3Gbps3Lanes).unwrap();
        phy.set_frl_rate(HdmiForumFrl::Rate12Gbps4Lanes).unwrap();
        assert_eq!(phy.frl_rate, Some(HdmiForumFrl::Rate12Gbps4Lanes));
    }

    #[test]
    fn set_frl_output_records_mode() {
        let mut phy = MockPhy::new();
        phy.set_frl_output(FrlOutput::GapOnly).unwrap();
        assert_eq!(phy.frl_output, Some(FrlOutput::GapOnly));
    }

    #[test]
    fn set_frl_output_can_be_switched() {
        let mut phy = MockPhy::new();
        phy.set_frl_output(FrlOutput::GapOnly).unwrap();
        phy.set_frl_output(FrlOutput::Active).unwrap();
        assert_eq!(phy.frl_output, Some(FrlOutput::Active));
    }

    #[test]
    fn value_types_can_be_hashed() {
        extern crate std;
        use std::collections::HashSet;
        let patterns: HashSet<LanePatterns> = [LanePatterns::default(), LanePatterns::default()]
            .into_iter()
            .collect();
        assert_eq!(patterns.len(), 1);
        let outputs: HashSet<FrlOutput> = [FrlOutput::GapOnly, FrlOutput::Active].into();
        assert_eq!(outputs.len(), 2);
        let levels: HashSet<TxFfeLevel> = [TxFfeLevel::MAX, TxFfeLevel::default()].into();
        assert_eq!(levels.len(), 2);
        let eq: HashSet<EqParams> = [EqParams::new(), EqParams::new()].into();
        assert_eq!(eq.len(), 1);
        let ltp: HashSet<LtpPattern> = [LtpPattern::Lfsr0, LtpPattern::Lfsr0].into();
        assert_eq!(ltp.len(), 1);
    }

    #[test]
    fn tx_ffe_level_accepts_0_to_7() {
        for level in 0..=7 {
            assert_eq!(TxFfeLevel::new(level).map(TxFfeLevel::value), Some(level));
        }
    }

    #[test]
    fn tx_ffe_level_rejects_above_7() {
        assert_eq!(TxFfeLevel::new(8), None);
        assert_eq!(TxFfeLevel::new(u8::MAX), None);
    }

    #[test]
    fn tx_ffe_level_max_and_default() {
        assert_eq!(TxFfeLevel::MAX.value(), 7);
        assert_eq!(TxFfeLevel::default().value(), 0);
        assert!(TxFfeLevel::default() < TxFfeLevel::MAX);
    }

    #[test]
    fn lane_eq_params_default_tx_ffe_level_is_0() {
        assert_eq!(LaneEqParams::default().tx_ffe_level, TxFfeLevel::default());
    }

    #[test]
    fn tx_ffe_restrictions_new_takes_the_bits_in_register_order() {
        let r = TxFfeRestrictions::new(true, false, false);
        assert!(r.pre_shoot_only && !r.de_emphasis_only && !r.no_ffe);
        let r = TxFfeRestrictions::new(false, true, false);
        assert!(!r.pre_shoot_only && r.de_emphasis_only && !r.no_ffe);
        let r = TxFfeRestrictions::new(false, false, true);
        assert!(!r.pre_shoot_only && !r.de_emphasis_only && r.no_ffe);
        // Combinations are carried as written; the PHY decides what they mean.
        let both = TxFfeRestrictions::new(true, true, false);
        assert!(both.pre_shoot_only && both.de_emphasis_only);
    }

    #[test]
    fn eq_params_restrict_nothing_by_default() {
        assert_eq!(
            TxFfeRestrictions::default(),
            TxFfeRestrictions::new(false, false, false)
        );
        assert_eq!(
            EqParams::new().tx_ffe_restrictions,
            TxFfeRestrictions::default()
        );
    }

    #[test]
    fn adjust_equalization_records_per_lane_levels() {
        let mut phy = MockPhy::new();
        let mut params = EqParams::new();
        params.lane1.tx_ffe_level = TxFfeLevel::MAX;
        phy.adjust_equalization(params).unwrap();
        let last = phy.last_eq.unwrap();
        assert_eq!(last, params);
        assert_eq!(last.lane0.tx_ffe_level.value(), 0);
        assert_eq!(last.lane1.tx_ffe_level.value(), 7);
    }

    #[test]
    fn adjust_equalization_tracks_call_count() {
        let mut phy = MockPhy::new();
        phy.adjust_equalization(EqParams::new()).unwrap();
        phy.adjust_equalization(EqParams::new()).unwrap();
        assert_eq!(phy.eq_calls, 2);
    }

    /// A minimal `TmdsPhy` that records what it is given, as `MockPhy` does for `FrlPhy`.
    /// Behaviour is tested where it lives: in spiritlevel, against its simulated PHY.
    struct MockTmdsPhy {
        config: Option<TmdsPhyConfig>,
        scrambling: Option<bool>,
        output: bool,
    }

    impl TmdsPhy for MockTmdsPhy {
        type Error = core::convert::Infallible;

        fn capabilities(&self) -> TmdsPhyCapabilities {
            TmdsPhyCapabilities::new(600_000)
                .with_scrambling(true)
                .with_low_rate_scrambling(true)
        }

        fn configure(&mut self, config: TmdsPhyConfig) -> Result<(), Self::Error> {
            self.config = Some(config);
            Ok(())
        }

        fn set_scrambling(&mut self, enabled: bool) -> Result<(), Self::Error> {
            self.scrambling = Some(enabled);
            Ok(())
        }

        fn start_output(&mut self) -> Result<(), Self::Error> {
            self.output = true;
            Ok(())
        }

        fn stop_output(&mut self) -> Result<(), Self::Error> {
            self.output = false;
            Ok(())
        }
    }

    const VIDEO_4K60: TmdsVideo = TmdsVideo::new(
        594_000,
        display_types::ColorBitDepth::Depth8,
        display_types::ColorFormat::Rgb444,
    );

    #[test]
    fn tmds_video_new_has_no_pixel_repetition() {
        assert_eq!(VIDEO_4K60.pixel_clock_khz, 594_000);
        assert_eq!(VIDEO_4K60.color_depth, display_types::ColorBitDepth::Depth8);
        assert_eq!(VIDEO_4K60.color_format, display_types::ColorFormat::Rgb444);
        assert_eq!(VIDEO_4K60.pixel_repetition, 0);
        assert_eq!(VIDEO_4K60.with_pixel_repetition(1).pixel_repetition, 1);
        // At run time as well as in a const, so coverage sees the constructor run.
        let video = TmdsVideo::new(
            594_000,
            display_types::ColorBitDepth::Depth8,
            display_types::ColorFormat::Rgb444,
        );
        assert_eq!(video, VIDEO_4K60);
    }

    #[test]
    fn tmds_phy_config_new_takes_its_fields_in_order() {
        const CONFIG: TmdsPhyConfig =
            TmdsPhyConfig::new(594_000, TmdsClockRatio::Div40, VIDEO_4K60);
        assert_eq!(CONFIG.character_rate_khz, 594_000);
        assert_eq!(CONFIG.clock_ratio, TmdsClockRatio::Div40);
        assert_eq!(CONFIG.video, VIDEO_4K60);
    }

    #[test]
    fn tmds_phy_capabilities_new_reports_no_scrambler() {
        let caps = TmdsPhyCapabilities::new(340_000);
        assert_eq!(caps.max_character_rate_khz, 340_000);
        assert!(!caps.scrambling);
        assert!(!caps.low_rate_scrambling);
        let caps = caps.with_scrambling(true).with_low_rate_scrambling(true);
        assert!(caps.scrambling && caps.low_rate_scrambling);
    }

    #[test]
    fn tmds_phy_records_configure_scrambling_and_output() {
        let mut phy = MockTmdsPhy {
            config: None,
            scrambling: None,
            output: false,
        };
        assert!(phy.capabilities().scrambling);
        let config = TmdsPhyConfig::new(594_000, TmdsClockRatio::Div40, VIDEO_4K60);
        phy.stop_output().unwrap();
        phy.configure(config).unwrap();
        phy.set_scrambling(true).unwrap();
        phy.start_output().unwrap();
        assert_eq!(phy.config, Some(config));
        assert_eq!(phy.scrambling, Some(true));
        assert!(phy.output);
        phy.set_scrambling(false).unwrap();
        phy.stop_output().unwrap();
        assert_eq!(phy.scrambling, Some(false));
        assert!(!phy.output);
    }

    /// A PHY with power, implementing both `TmdsPhy` and `PhyPower` with one error type, as
    /// the trait's documentation allows.
    struct PoweredPhy {
        tmds: MockTmdsPhy,
        powered: bool,
    }

    impl PhyPower for PoweredPhy {
        type Error = core::convert::Infallible;

        fn power_on(&mut self) -> Result<(), Self::Error> {
            self.powered = true;
            Ok(())
        }

        fn power_off(&mut self) -> Result<(), Self::Error> {
            self.powered = false;
            self.tmds.output = false;
            self.tmds.config = None;
            Ok(())
        }
    }

    impl TmdsPhy for PoweredPhy {
        type Error = <Self as PhyPower>::Error;

        fn capabilities(&self) -> TmdsPhyCapabilities {
            self.tmds.capabilities()
        }

        fn configure(&mut self, config: TmdsPhyConfig) -> Result<(), Self::Error> {
            self.tmds.configure(config)
        }

        fn set_scrambling(&mut self, enabled: bool) -> Result<(), Self::Error> {
            self.tmds.set_scrambling(enabled)
        }

        fn start_output(&mut self) -> Result<(), Self::Error> {
            self.tmds.start_output()
        }

        fn stop_output(&mut self) -> Result<(), Self::Error> {
            self.tmds.stop_output()
        }
    }

    /// A caller that powers the PHY bounds on both traits; it drives every `TmdsPhy` method,
    /// so the forwarding to the inner mock is checked too.
    fn power_cycle<P: PhyPower + TmdsPhy>(phy: &mut P, config: TmdsPhyConfig) {
        phy.power_off().unwrap();
        phy.power_off().unwrap();
        phy.power_on().unwrap();
        phy.power_on().unwrap();
        phy.stop_output().unwrap();
        phy.configure(config).unwrap();
        let scrambling = phy.capabilities().scrambling;
        phy.set_scrambling(scrambling).unwrap();
        phy.start_output().unwrap();
    }

    #[test]
    fn phy_power_composes_with_a_mode_trait() {
        let mut phy = PoweredPhy {
            tmds: MockTmdsPhy {
                config: None,
                scrambling: None,
                output: true,
            },
            powered: true,
        };
        let config = TmdsPhyConfig::new(594_000, TmdsClockRatio::Div40, VIDEO_4K60);
        power_cycle(&mut phy, config);
        assert!(phy.powered && phy.tmds.output);
        assert_eq!(phy.tmds.config, Some(config));
        assert_eq!(phy.tmds.scrambling, Some(true));
        phy.power_off().unwrap();
        assert!(!phy.powered && !phy.tmds.output);
        assert_eq!(phy.tmds.config, None);
    }
}
