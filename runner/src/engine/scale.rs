//! Clock scaling, as OpenBench does it: a test's clock is stretched or shrunk to this machine's speed, so a test at
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
//!   the same build), or the mean of the two factors for BOTH;
//! - multiplies the clock's base time and increment by it and passes them to the match runner with two decimals
//!   (`tc=%.2f+%.2f timemargin=250`); node and depth limits are not scaled.
//!
//! This does the same, once per test (stages run at different times, so each measures the machine as it is then) and
//! each time a tune starts or resumes.

use std::{path::Path, time::Duration};

use anyhow::{Context, Result, bail};
use serde::{Deserialize, Serialize};
use tokio_util::sync::CancellationToken;
use utoipa::ToSchema;

use super::build;
use crate::{config::Scaling, funnel::TimeControl, proc};

/// How a test's or a tune's clock was scaled.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, ToSchema)]
pub struct Calibration {
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
    /// The clock fastchess was given, rounded to hundredths of a second as OpenBench passes it.
    pub played: TimeControl,
}

/// `reference / measured`.
pub fn factor(reference_nps: u64, measured_nps: u64) -> f64 {
    reference_nps as f64 / measured_nps.max(1) as f64
}

/// A clock scaled by `factor`, to hundredths of a second (OpenBench's `%.2f`). Node limits stay as they are.
pub fn scale(tc: TimeControl, factor: f64) -> TimeControl {
    let hundredths = |s: f64| (s * factor * 100.0).round() / 100.0;
    match tc {
        TimeControl::Clock { seconds, increment } => {
            TimeControl::Clock { seconds: hundredths(seconds), increment: hundredths(increment) }
        }
        nodes => nodes,
    }
}

/// Whether a time control is scaled at all under these settings.
pub fn applies(tc: TimeControl, settings: Scaling) -> bool {
    settings.enabled && matches!(tc, TimeControl::Clock { .. })
}

/// How long the benches may take together before the machine is judged stuck. OpenBench allows 60 s per set; a
/// heavily loaded machine running 15 copies of Reckless's bench is slower than one copy, so this is generous.
const BENCH_TIMEOUT: Duration = Duration::from_secs(300);

/// Runs `copies` of `bin bench` at once and returns their mean speed. Every copy must search `expected` nodes, when
/// given (a build whose bench changes under load is not deterministic). A cancel kills them all.
pub async fn measure(bin: &Path, copies: u32, expected: Option<u64>, cancel: &CancellationToken) -> Result<u64> {
    let mut children = Vec::new();
    for _ in 0..copies.max(1) {
        children.push(proc::spawn(proc::Spawn { program: bin, args: &["bench"], cwd: None, env: &[], stdin: false })?);
    }
    // Each child's output to the end; the children stay owned here, so dropping this future (a cancel, the timeout)
    // kills every one of them.
    let outputs = futures::future::try_join_all(children.iter_mut().map(|child| {
        let mut lines = proc::lines(child);
        async move {
            let mut out = String::new();
            while let Some(line) = lines.recv().await {
                out.push_str(&line);
                out.push('\n');
            }
            anyhow::Ok(out)
        }
    }));
    let outputs = tokio::select! {
        out = tokio::time::timeout(BENCH_TIMEOUT, outputs) => out.context("bench took too long")??,
        _ = cancel.cancelled() => bail!("cancelled"),
    };
    let mut speeds = Vec::new();
    for out in &outputs {
        let (nodes, nps) = build::parse_bench(out).with_context(|| format!("no Bench line in: {out}"))?;
        if let Some(expected) = expected.filter(|e| *e != nodes) {
            bail!("{} benched {nodes} nodes with others running, not {expected}: not deterministic", bin.display());
        }
        speeds.push(nps);
    }
    for child in &mut children {
        child.wait().await?;
    }
    Ok(speeds.iter().sum::<u64>() / speeds.len() as u64)
}

/// One build's side of a calibration: its binary and the node count its bench must give.
pub struct Side<'a> {
    pub bin: &'a Path,
    pub bench: u64,
}

/// Benches base and then dev (or the one build of a tune) with `copies` at once and scales `tc` by base's speed.
pub async fn calibrate(
    tc: TimeControl,
    settings: Scaling,
    base: Side<'_>,
    dev: Option<Side<'_>>,
    copies: u32,
    cancel: &CancellationToken,
    log: &(dyn Fn(&str) + Sync),
) -> Result<Calibration> {
    log(&format!("calibrating the clock: {copies} copies of bench at once"));
    let base_nps = measure(base.bin, copies, Some(base.bench), cancel).await?;
    let dev_nps = match dev {
        Some(dev) => measure(dev.bin, copies, Some(dev.bench), cancel).await?,
        None => base_nps,
    };
    let c = calibration(tc, settings.reference_nps, base_nps, dev_nps, copies);
    log(&format!(
        "base {base_nps} nps, dev {dev_nps} nps, reference {}: clock ×{:.3}, played as {}",
        c.reference_nps,
        c.scale,
        clock(c.played)
    ));
    Ok(c)
}

/// The record of a calibration from its measured speeds.
pub fn calibration(tc: TimeControl, reference_nps: u64, base_nps: u64, dev_nps: u64, copies: u32) -> Calibration {
    let scale_by = factor(reference_nps, base_nps);
    Calibration {
        copies,
        dev_nps,
        base_nps,
        measured_nps: base_nps,
        reference_nps,
        scale: scale_by,
        played: scale(tc, scale_by),
    }
}

/// A time control as people write it: `8+0.08`, `25000 nodes`.
pub fn clock(tc: TimeControl) -> String {
    match tc {
        TimeControl::Clock { seconds, increment } => format!("{seconds}+{increment}"),
        TimeControl::Nodes { nodes } => format!("{nodes} nodes"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const STC: TimeControl = TimeControl::Clock { seconds: 8.0, increment: 0.08 };

    #[test]
    fn the_factor_is_reference_over_measured() {
        assert!((factor(660_000, 480_000) - 1.375).abs() < 1e-12);
        assert!((factor(660_000, 660_000) - 1.0).abs() < 1e-12);
        assert!(factor(660_000, 0).is_finite());
    }

    #[test]
    fn scales_base_and_increment_to_hundredths() {
        assert_eq!(scale(STC, 1.375), TimeControl::Clock { seconds: 11.0, increment: 0.11 });
        // 8 × 660/212 = 24.905…, 0.08 × 660/212 = 0.249…
        assert_eq!(scale(STC, factor(660_000, 212_000)), TimeControl::Clock { seconds: 24.91, increment: 0.25 });
        // A faster machine gets a shorter clock.
        assert_eq!(scale(STC, factor(660_000, 1_320_000)), TimeControl::Clock { seconds: 4.0, increment: 0.04 });
    }

    #[test]
    fn leaves_node_limits_alone() {
        let nodes = TimeControl::Nodes { nodes: 25_000 };
        assert_eq!(scale(nodes, 3.0), nodes);
        let on = Scaling { enabled: true, reference_nps: 660_000 };
        assert!(!applies(nodes, on));
        assert!(applies(STC, on));
        assert!(!applies(STC, Scaling { enabled: false, ..on }));
    }

    #[test]
    fn records_base_speed_as_the_measure() {
        let c = calibration(STC, 660_000, 480_000, 500_000, 7);
        assert_eq!((c.measured_nps, c.dev_nps, c.copies), (480_000, 500_000, 7));
        assert_eq!(c.played, TimeControl::Clock { seconds: 11.0, increment: 0.11 });
        assert_eq!(clock(c.played), "11+0.11");
    }

    #[test]
    fn reads_the_settings() {
        let default = Scaling::parse(None, None);
        assert_eq!(default, Scaling { enabled: true, reference_nps: crate::config::REFERENCE_NPS });
        assert!(!Scaling::parse(Some("off"), None).enabled);
        assert!(!Scaling::parse(Some("0"), None).enabled);
        assert!(Scaling::parse(Some("on"), None).enabled);
        assert_eq!(Scaling::parse(None, Some("500000")).reference_nps, 500_000);
        assert_eq!(Scaling::parse(None, Some("fast")).reference_nps, crate::config::REFERENCE_NPS);
    }

    /// A real bench, with a build from the runner's data folder named by `RECKLESS_BIN`.
    #[tokio::test]
    #[ignore = "needs a Reckless binary in RECKLESS_BIN"]
    async fn measures_a_real_build() {
        let bin = std::env::var("RECKLESS_BIN").expect("RECKLESS_BIN");
        let copies = std::env::var("COPIES").ok().and_then(|c| c.parse().ok()).unwrap_or(2);
        let nps = measure(Path::new(&bin), copies, None, &CancellationToken::new()).await.unwrap();
        println!("{copies} copies: {nps} nps, factor {:.3}", factor(crate::config::REFERENCE_NPS, nps));
        assert!(nps > 0);

        // A cancel stops them at once (and dropping the children kills them).
        let token = CancellationToken::new();
        let cancel = token.clone();
        tokio::spawn(async move {
            tokio::time::sleep(Duration::from_millis(500)).await;
            cancel.cancel();
        });
        let start = std::time::Instant::now();
        assert!(measure(Path::new(&bin), copies, None, &token).await.is_err());
        assert!(start.elapsed() < Duration::from_secs(2));
    }
}
