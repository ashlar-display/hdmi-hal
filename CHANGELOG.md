# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

### Breaking changes

- **`ScdcTransport::read_block` is a required method.** Its default implementation (one
  `read` per byte) is gone: a transport whose author never considered block reads would
  read related registers, such as the CED counters, one at a time and could return values
  torn by a sink update, with nothing to show for it. A transport that reads several
  registers in one DDC transaction implements `read_block` with it; one that cannot adds
  `use hdmi_hal::scdc::read_block_bytewise;` and
  `fn read_block(&self, reg: u8, buf: &mut [u8]) -> Result<(), Self::Error> {
  read_block_bytewise(self, reg, buf) }`, which behaves as the old default did.
- **`read_block`'s contract is written down, and it is stricter than the old default.**
  An existing override must be checked against it: the whole buffer as one combined
  transfer to the layer below, or an error — never split, never a fallback to one
  register at a time (a bus driver known to split it is named in the transport's docs);
  no consistent snapshot promised (that is the sink's); a maximum length, if any,
  documented and of at least 16 bytes, above which a read is an error; an empty buffer is
  `Ok` with no bus activity; on error the buffer is unspecified; a read past `0xFF` is
  unspecified but never panics. The old default's wrap to `0x00` past `0xFF` is no
  longer promised (`read_block_bytewise` still wraps).

### Added

- **`scdc::read_block_bytewise(transport, reg, buf)`** — `read_block` as one `read` per
  byte, for transports that cannot burst-read; the documented exception to the "one
  transaction" rule. Generic over `T: ScdcTransport + ?Sized`.
- **"Errors" on `ScdcTransport`:** after an error an implementation should stay usable for
  the next call; one that cannot documents how to recover.

### Changed

- **Part of Ashlar** — the README now opens with a block naming the
  [Ashlar](https://ashlar-display.org) stack and linking the other crates in it. Keywords
  are aligned across the stack, with `ashlar` and `hdmi` on every crate; `ashlar` is added,
  `display` is dropped. Categories gain `no-std::no-alloc`. The crate's `homepage` is now
  the Ashlar website.

- **`TxFfeRestrictions`' documentation corrected and completed.** 0.5.1 said the
  reference drivers act on the bits while the sink requests the DDE compliance pattern
  (LTP 4); only the Rockchip driver does, the AMD driver applies them with every TxFFE
  setting. The contract now also says that no bit set means unrestricted TxFFE for every
  backend (the Rockchip reading, where bits enable components, applies only once a bit is
  set), that a PHY unable to restrict its TxFFE may ignore the bits if it says so, and
  that AMD's "both only bits" leaves neither component without being identical to its
  `no_ffe` setting. No API change.
- **Moved to the `ashlar-display` organization** — the repository is now
  `ashlar-display/hdmi-hal` (the old URLs redirect), and `repository` points there.
  Releases are attested and published from the new repository; 0.4.0 to 0.5.1, attested
  before the move, verify with `--owner DracoWhitefire --signer-repo
  DracoWhitefire/hdmi-hal` (see "Verifying releases" in the README). The code of conduct is
  now the organization's, shared by every Ashlar repository; the repository's own copy is
  removed.

## [0.5.1] - 2026-10-12

### Added

- **`TxFfeRestrictions` and `EqParams::tx_ffe_restrictions`**: the three TxFFE bits a
  compliance tester can set in `Source_Test_Configuration` (`TxFFE_Pre_Shoot_Only`,
  `TxFFE_De_Emphasis_Only`, `TxFFE_No_FFE`), passed to the PHY for the whole link with
  every `adjust_equalization`. They are carried as written; how to combine them is the
  PHY's to decide and document, because the reference drivers disagree (AMD: both "only"
  bits mean no FFE, and no FFE overrides; Rockchip: both mean full FFE, and no FFE is not
  read). The default restricts nothing, so existing `HdmiPhy` implementations keep
  compiling and behave as before; `EqParams` is `#[non_exhaustive]`, so the field is
  additive.

### Changed

- `EqParams`'s documentation (and `doc/architecture.md`) no longer says its levels are
  derived from CED feedback: they follow the sink's FFE-change requests (0xE) during link
  training.
- `doc/roadmap.md` no longer lists released versions (it stopped at 0.3.0); this changelog
  is the record of what each release contains, and the roadmap covers what comes next.

## [0.5.0] - 2026-10-11

### Breaking changes

- **`display-types` updated to 0.4** (0.4.0 used 0.3): the `HdmiForumFrl` in
  `HdmiPhy::set_frl_rate` is display-types 0.4's type, so PHY implementations and their
  callers must use display-types 0.4 (or the `HdmiForumFrl` re-exported here). This change
  first shipped in 0.4.1, which was yanked for releasing it as a patch.
- **`LtpPattern` is now a `#[non_exhaustive]` enum with the HDMI 2.1 pattern values.**
  The previous documentation mapped 1–4 to LFSR 0–3, which was wrong: the values are
  1 = all ones, 2 = all zeros, 3 = Nyquist clock, 4 = DDE compliance and 5–8 = LFSR 0–3.
  `LtpPattern::new(u8)` is removed; use the variants (`LtpPattern::Lfsr0` etc.).
  `LtpPattern::value()` remains and is now `const`. The enum is `#[repr(u8)]`, with each
  variant's discriminant its pattern value.
- **`HdmiPhy::send_ltp` takes the full per-lane pattern set**: the signature is now
  `send_ltp(patterns: LanePatterns)`. HDMI 2.1 sinks request a pattern per lane, so a
  single pattern for all lanes could not express training. `None` on a lane means no
  training pattern; `None` on every lane stops the training patterns.
- **`HdmiPhy::set_frl_output` is a new required method**: existing `HdmiPhy`
  implementations must add `set_frl_output(output: FrlOutput) -> Result<(), Self::Error>`.
  It selects gap-only output during link training and active output once training has
  succeeded.

### Added

- `HdmiForumFrl` re-exported at the crate root, so `HdmiPhy` implementations name the
  same type hdmi-hal does without depending on `display-types` themselves.
- `LtpPattern`, `LanePatterns`, `FrlOutput`, `TxFfeLevel`, `LaneEqParams` and `EqParams`
  derive `Hash`.
- `LanePatterns` — the link training pattern for each lane (`lane0` to `lane3`, each an
  `Option<LtpPattern>`; `lane3` is `None` in 3-lane FRL mode). Derives `Default` (no
  pattern on any lane), `Debug`, `Clone`, `Copy`, `PartialEq` and `Eq`.
- `FrlOutput` — what the transmitter sends on the FRL lanes: `GapOnly` (gap characters
  only) or `Active` (video, data islands and control periods). Derives `Debug`, `Clone`,
  `Copy`, `PartialEq` and `Eq`. Deliberately exhaustive: a PHY has to implement both
  modes, so a mode added by a later HDMI version would be a breaking change.
- `TxFfeLevel` — a TxFFE level index, 0–7. `TxFfeLevel::new(u8)` returns `None` above 7;
  `TxFfeLevel::MAX` is level 7 and the default is level 0. Read with `value()`. Derives
  `Debug`, `Clone`, `Copy`, `Default`, `PartialEq`, `Eq`, `PartialOrd` and `Ord`, so levels
  compare in order.
- `LaneEqParams::tx_ffe_level` — the lane's TxFFE level, applied through
  `HdmiPhy::adjust_equalization`. Defaults to level 0.
- `EqParams` and `LaneEqParams` now derive `PartialEq` and `Eq`.
- `ScdcTransport::read_block(reg, buf)` — reads consecutive registers into `buf`. The
  default implementation calls `read` once per byte, so existing transports need no
  changes; transports that can burst-read should override it.

### Changed

- `HdmiPhy`'s documentation now describes what it covers: what FRL link training needs
  from the transmitter (rate, training patterns, output mode, scrambling and each lane's
  TxFFE level). Further PHY configuration (lane mapping, TMDS pre-emphasis and swing,
  spread-spectrum clocking, power states) is listed as planned in the roadmap.
- `HdmiPhy::set_frl_rate`'s documentation says it returns once the PHY transmits at the
  rate, including any bring-up its hardware needs (such as a clock pattern held until its
  PLL locks).

### Fixed

- **hdmi-hal now builds for `no_std` targets.** It depended on `display-types` with its
  default features, which include `std`, so `std` was enabled for display-types in every
  crate depending on hdmi-hal, whatever that crate asked for, and the build failed on
  targets without `std` (such as `thumbv7em-none-eabi`). display-types is now a
  dependency with `default-features = false`.

### Internal

- **CI builds for a `no_std` target** — the `Build (no_std)` step now builds for
  `thumbv7em-none-eabi`. It previously built for the host, where `std` is always
  available, so it could not catch a dependency that requires `std`.
  The publish workflow runs the same build steps.
- **Publish dispatches restricted to release tags** — `publish.yml` already accepted
  `workflow_dispatch` (used by `release-tag`), but a dispatch against a branch such as
  `main` would have published the version on that branch and created a GitHub release
  named after the branch. Dispatches against a non-tag ref are now skipped, so they cannot
  publish or create a release.
- **Publishing uses crates.io trusted publishing** — `publish.yml` obtains a short-lived
  token through OIDC (`rust-lang/crates-io-auth-action`, revoked at the end of the job, and
  short-lived regardless) instead of the long-lived `CARGO_REGISTRY_TOKEN` secret. The
  upload job runs in the `release` environment, which only `v*` tags can deploy to and
  which requires a maintainer's approval. Quality gates moved to a separate `verify` job,
  so no dependency build scripts run and no cache is restored where publishing credentials
  exist. The attested `.crate` is kept as a separate copy; after upload the job fails if
  crates.io's checksum differs from it, and that same copy is attached to the GitHub
  release. Re-running a partially failed publish reuses an existing GitHub release.
- **Release summary before approval** — `publish.yml` gains a `summary` job that shows the
  crate, version and commit, changes since the previous published release (ignoring
  pre-release or malformed tags, and flagging a baseline that is not on crates.io), changes
  to workflow, toolchain and build configuration, manifest changes, the changelog section,
  and the `.crate` sha256 with a command to reproduce it, on the run page while the publish
  job waits for approval. The publish job refuses to attest a package whose sha256 differs
  from the reviewed one.
- **`release-tag` no longer tags after a failed promotion** — `release-tag` now promotes
  `develop` to `main` through a `release-promote/v*` branch and PR. A failed merge of the promote PR used to go unnoticed, after which the previous `main`
  commit was tagged with the new version. The step now fails, tagging requires `main` to
  hold exactly the promoted tree, and `publish.yml` refuses tags that do not match the
  `Cargo.toml` version.
- **Release workflows only run from `develop`** — `release-prep` and `release-tag` fail when
  started from another branch, so an outdated copy of the workflow is not run against
  `develop` by mistake.
- **`release-prep` no longer edits the README** — its "Update README.md dependency pin" step
  matched nothing since the README switched to `cargo add hdmi-hal`, and was removed.
- **No CI run on promotion PRs** — CI no longer runs on pull requests into `main`. The only
  ones are `release-tag`'s `release-promote/*` PRs, whose content already passed CI on
  `develop`; opened by `github-actions[bot]`, their runs were held for approval and listed
  as `action_required` or failed.
- **Supply-chain hardening for CI** — every third-party action is pinned to a commit SHA
  of a release tag, with `dependabot.yml` keeping the pins (and `Cargo.lock`) current. The
  Rust toolchain itself still follows `stable`. `cargo-semver-checks` is installed as a
  checksum-verified prebuilt binary (the version listed by the pinned `install-action`)
  instead of being restored from a cached `~/.cargo/bin`. The security audit also runs
  weekly.
- **README documents release provenance** — a badge shows whether crates.io requires trusted
  publishing for this crate, and "Verifying releases" now states which versions carry SLSA
  provenance (from 0.4.0, not every release as it previously claimed), which are uploaded
  through trusted publishing (after 0.4.1) or were uploaded with an API token (up to
  0.4.1), and how to look up a version's publishing details on crates.io.

## [0.4.1] - 2026-05-20 [YANKED]

Yanked: moving to display-types 0.4 changed the `HdmiForumFrl` type in `HdmiPhy`'s
signature, a breaking change released as a patch. Crates that depend on hdmi-hal 0.4 and
display-types 0.3, such as plumbob 0.1.3, failed to build against it. The change is
released again in 0.5.

### Changed

- **`display-types` dependency bumped from `0.3.1` to `0.4`.**

### Internal

- `examples/simulate/Cargo.lock` is no longer tracked in git and is now listed in
  `.gitignore`, and `--locked` has been dropped from the CI build of the simulate example.
  The simulate example is `publish = false`; committing its lock file provided no
  reproducibility benefit and required manual updates on every version bump.

## [0.4.0] - 2026-04-13

### Breaking changes

- **`ScdcTransport::read` now takes `&self` instead of `&mut self`.** Implementations
  that previously declared `fn read(&mut self, reg: u8)` must change the receiver to
  `&self`. Implementations that mutated internal state during reads (e.g. operation
  counters, offset tracking) must introduce interior mutability (`Cell`, `Mutex`, etc.)
  for those fields. The `write` method is unchanged and still takes `&mut self`.

  *Motivation:* register reads are logically non-mutating. The `&mut self` receiver
  was an implementation leak from `std::fs::File` requiring `&mut` for I/O, not a
  semantic requirement of the trait. Changing to `&self` allows transport references
  to be shared across concurrent read operations without a `Mutex` wrapper.

### Added

- **SLSA Build Level 2 provenance** — release artifacts are attested via
  `actions/attest-build-provenance` and verified with
  `gh attestation verify <file> --repo DracoWhitefire/hdmi-hal`.

## [0.3.0] - 2026-04-03

### Breaking changes

- **`EqParams` now has per-lane fields**: `lane0`, `lane1`, `lane2` (`LaneEqParams`)
  and `lane3` (`Option<LaneEqParams>`, `None` in 3-lane FRL mode). Implementations of
  `HdmiPhy::adjust_equalization` that previously ignored the empty struct should be
  updated to read the per-lane fields. `EqParams::new()` and `EqParams::default()`
  construct a valid zero-valued instance and are unchanged.

### Added

- `LaneEqParams` — per-lane equalization parameter struct, `#[non_exhaustive]`.
  Fields will be defined as the link training layer is implemented.
- `EqParams` and `LaneEqParams` now derive `Debug`, `Clone`, and `Copy`.

## [0.2.0] - 2026-04-03

### Breaking changes

- **`HdmiPhy::send_ltp` is a new required method**: existing `HdmiPhy` implementations
  must add `send_ltp(pattern: LtpPattern) -> Result<(), Self::Error>`. The method drives
  the link training pattern requested by the sink on the physical lanes during FRL
  training. The raw LFSR index (1 = LFSR0, 2 = LFSR1, 3 = LFSR2, 4 = LFSR3) is
  available via `LtpPattern::value()`.

### Added

- `LtpPattern` — newtype carrying a link training pattern index to be driven on the
  physical lanes. Constructed by link training state machines via `LtpPattern::new(u8)`;
  consumed by PHY backends via `LtpPattern::value() -> u8`. Raw value matches the SCDC
  Status_Flags encoding: 1 = LFSR0, 2 = LFSR1, 3 = LFSR2, 4 = LFSR3.
- `HdmiPhy::send_ltp(pattern: LtpPattern)` — drives the requested link training pattern
  on the physical lanes. Called by the FRL training loop on each iteration where the sink
  requests a non-zero pattern.

### Internal

- Unit tests for `HdmiPhy` and `EqParams` via a `MockPhy` implementation in `phy::tests`.
- Coverage ratchet CI job: measures line coverage with `cargo-llvm-cov`, checks against
  `.coverage-baseline`, and opens an automatic PR to advance the baseline when coverage improves.
- Fixed `.coverage-baseline`: value had been written with a locale comma separator
  (`100,00`); corrected to `100.00`.
- Fixed coverage ratchet CI: added `LC_NUMERIC=C` to the baseline `printf` to prevent
  locale-dependent decimal separators from corrupting `.coverage-baseline` on non-C locales.

## [0.1.0] - 2026-03-29

### Added

- `ScdcTransport` — raw register read/write access to the SCDC register map over
  DDC/I²C. Single-byte `read(reg: u8) -> Result<u8, Self::Error>` and
  `write(reg: u8, value: u8) -> Result<(), Self::Error>`. Associated `Error` type
  bounded by the implementing crate.
- `HdmiPhy` — PHY lane configuration for an HDMI 2.1 transmitter or receiver.
  Methods: `set_frl_rate(HdmiForumFrl)`, `adjust_equalization(EqParams)`, and
  `set_scrambling(bool)`. Associated `Error` type bounded by the implementing crate.
- `EqParams` — placeholder struct carrying equalization parameters passed from link
  training feedback to the PHY. Fields to be defined as the link training layer is
  implemented. Marked `#[non_exhaustive]`.
- `simulate` example — worked implementation of both traits against an in-memory
  register array, demonstrating the backend and consumer patterns downstream crates
  will follow.

**Project infrastructure**

- `#![no_std]`, `#![forbid(unsafe_code)]`
- `#[non_exhaustive]` on all public structs
- Full rustdoc coverage enforced via `cargo rustdoc -- -D missing_docs`
- CI workflow: fmt, clippy, docs
- Publish workflow: triggered on version tags, gated to commits reachable from `main`
