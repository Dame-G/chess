//! Clock scaling, as OpenBench does it: a trial's clock is stretched or shrunk to this machine's speed, so a trial at
//! "8+0.08" searches about as many nodes per move here as it does on Reckless's own OpenBench workers.
//!
//! OpenBench's worker (`Client/worker.py`, `determine_scale_factor` and `scale_time_control`; `Client/bench.py`,
//! `run_benchmark`), before each workload:
//!
//! - runs `bench` on dev and on base, as many copies at once as the worker has threads (one per game slot for
//!   single-threaded tests), once, and takes the mean of the copies' nodes per second; every copy must report the
//!   expected node count;
//! - takes the factor `scale_nps / nps`: from base's speed for tests (`scale_method` BASE, the default, and what
//!   Reckless's instance uses: its tests read "Scaling BASE 660000 NPS"), from dev's for tunes (where dev and base are
//!   the same build);
//! - multiplies the clock's base time and increment by it and passes them to the match runner with two decimals
//!   (`tc=%.2f+%.2f timemargin=250`); node and depth limits are not scaled.
//!
//! This module is the arithmetic and the settings; running the benches is the engine adapter's job.

use serde::{Deserialize, Serialize};
use utoipa::ToSchema;

use crate::time::TimeControl;

/// The speed Reckless's OpenBench scales its clocks to: `bench` nodes per second, measured with every slot busy.
pub const REFERENCE_NPS: u64 = 660_000;

/// How a trial's or a tune's clock was scaled.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize, ToSchema)]
pub struct Scaling {
    /// Copies of `bench` run at once: the games played at once times each game's threads.
    pub copies: u32,
    /// The mean speed of the copies of dev's `bench`, in nodes per second.
    pub dev_nps: u64,
    /// The mean speed of the copies of base's `bench`. A tune has one build, so both are its speed.
    pub base_nps: u64,
    /// The speed the factor comes from: base's, as Reckless's OpenBench scales its tests.
    pub measured_nps: u64,
    /// The speed the written clock is meant for.
    pub reference_nps: u64,
    /// `reference_nps / measured_nps`: what the clock's base time and increment were multiplied by.
    pub scale: f64,
}

/// `reference / measured`.
pub fn factor(reference_nps: u64, measured_nps: u64) -> f64 {
    reference_nps as f64 / measured_nps.max(1) as f64
}

/// A clock scaled by `factor`, to hundredths of a second (OpenBench's `%.2f`). Other time controls stay as they are.
pub fn scale(tc: TimeControl, factor: f64) -> TimeControl {
    let hundredths = |ms: u64| ((ms as f64 * factor / 10.0).round() as u64) * 10;
    match tc {
        TimeControl::Clock { initial_ms, increment_ms } => {
            TimeControl::Clock { initial_ms: hundredths(initial_ms), increment_ms: hundredths(increment_ms) }
        }
        other => other,
    }
}

/// The record of a calibration from its measured speeds, and the clock to play: the factor comes from base's speed.
/// A tune passes its one build's speed as both.
pub fn calibrate(
    tc: TimeControl,
    reference_nps: u64,
    base_nps: u64,
    dev_nps: u64,
    copies: u32,
) -> (Scaling, TimeControl) {
    let scale_by = factor(reference_nps, base_nps);
    let scaling = Scaling { copies, dev_nps, base_nps, measured_nps: base_nps, reference_nps, scale: scale_by };
    (scaling, scale(tc, scale_by))
}

/// Whether and how trials and tunes scale their clocks, from `DAMELESS_SCALE_CLOCKS` and `DAMELESS_REFERENCE_NPS`.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize, ToSchema)]
pub struct ScalingSettings {
    pub enabled: bool,
    pub reference_nps: u64,
}

impl Default for ScalingSettings {
    fn default() -> Self {
        Self { enabled: true, reference_nps: REFERENCE_NPS }
    }
}

impl ScalingSettings {
    /// From the two variables' values, as the caller read them: on unless `enabled` is `off`, `0`, `false` or `no`;
    /// the reference is Reckless's unless a positive number.
    pub fn parse(enabled: Option<&str>, reference_nps: Option<&str>) -> Self {
        let off =
            enabled.is_some_and(|v| matches!(v.trim().to_ascii_lowercase().as_str(), "off" | "0" | "false" | "no"));
        let reference_nps =
            reference_nps.and_then(|v| v.trim().parse().ok()).filter(|n| *n > 0).unwrap_or(REFERENCE_NPS);
        Self { enabled: !off, reference_nps }
    }

    /// Whether a time control is scaled at all under these settings: only clocks, and only when on.
    pub fn applies(&self, tc: &TimeControl) -> bool {
        self.enabled && tc.is_clock()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const STC: TimeControl = TimeControl::Clock { initial_ms: 8_000, increment_ms: 80 };

    #[test]
    fn the_factor_is_reference_over_measured() {
        assert!((factor(660_000, 480_000) - 1.375).abs() < 1e-12);
        assert!((factor(660_000, 660_000) - 1.0).abs() < 1e-12);
        assert!(factor(660_000, 0).is_finite());
    }

    #[test]
    fn scales_base_and_increment_to_hundredths() {
        assert_eq!(scale(STC, 1.375), TimeControl::clock(11_000, 110));
        // 8 × 660/212 = 24.905…, 0.08 × 660/212 = 0.249…
        assert_eq!(scale(STC, factor(660_000, 212_000)), TimeControl::clock(24_910, 250));
        assert_eq!(scale(STC, factor(660_000, 212_000)).seconds_text(), "24.91+0.25");
        // A faster machine gets a shorter clock.
        assert_eq!(scale(STC, factor(660_000, 1_320_000)), TimeControl::clock(4_000, 40));
    }

    #[test]
    fn leaves_node_limits_alone() {
        let nodes = TimeControl::Nodes { nodes: 25_000 };
        assert_eq!(scale(nodes, 3.0), nodes);
        let on = ScalingSettings { enabled: true, reference_nps: 660_000 };
        assert!(!on.applies(&nodes));
        assert!(on.applies(&STC));
        assert!(!ScalingSettings { enabled: false, ..on }.applies(&STC));
    }

    #[test]
    fn records_base_speed_as_the_measure() {
        let (c, played) = calibrate(STC, 660_000, 480_000, 500_000, 7);
        assert_eq!((c.measured_nps, c.dev_nps, c.copies), (480_000, 500_000, 7));
        assert_eq!(played, TimeControl::clock(11_000, 110));
        assert_eq!(played.seconds_text(), "11+0.11");
    }

    #[test]
    fn reads_the_settings() {
        assert_eq!(ScalingSettings::parse(None, None), ScalingSettings { enabled: true, reference_nps: REFERENCE_NPS });
        assert!(!ScalingSettings::parse(Some("off"), None).enabled);
        assert!(!ScalingSettings::parse(Some("0"), None).enabled);
        assert!(!ScalingSettings::parse(Some(" No "), None).enabled);
        assert!(ScalingSettings::parse(Some("on"), None).enabled);
        assert_eq!(ScalingSettings::parse(None, Some("500000")).reference_nps, 500_000);
        assert_eq!(ScalingSettings::parse(None, Some("fast")).reference_nps, REFERENCE_NPS);
        assert_eq!(ScalingSettings::parse(None, Some("0")).reference_nps, REFERENCE_NPS);
    }
}
