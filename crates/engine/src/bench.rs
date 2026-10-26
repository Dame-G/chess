//! Measuring a build's speed as OpenBench does before a test: `copies` of `bench` at once (as many as the threads the
//! games will use), the mean of their nodes per second. Scaling a clock by it is the domain's maths; this only measures.
//!
//! Every copy should search the same number of nodes. A copy that does not (the build is not deterministic under
//! load, or the expected count is stale) is flagged, not hidden: the caller decides whether that fails the trial.

use std::{path::Path, time::Duration};

use anyhow::{Context, Result, bail};
use serde::{Deserialize, Serialize};
use tokio_util::sync::CancellationToken;

use crate::{Cancelled, build, proc};

/// How long the copies may take together before the machine is judged stuck. OpenBench allows 60 s per set; a loaded
/// machine running fifteen copies of Reckless's bench is slower than one, so this is generous.
pub const BENCH_TIMEOUT: Duration = Duration::from_secs(300);

/// One copy's answer.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct BenchCopy {
    pub nodes: u64,
    pub nps: u64,
    /// Its node count differs from the expected one (or, with none given, from the most common one).
    pub differs: bool,
}

/// A measurement of one build.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Measurement {
    pub copies: Vec<BenchCopy>,
    /// The mean speed of the copies, in nodes per second.
    pub mean_nps: u64,
    /// The node count every copy should have given.
    pub nodes: u64,
}

impl Measurement {
    /// Whether every copy searched the same nodes.
    pub fn deterministic(&self) -> bool {
        self.copies.iter().all(|c| !c.differs)
    }

    /// Builds the record from the copies' answers, flagging those whose nodes differ from `expected` (or, without one,
    /// from the most common count).
    pub fn from_benches(benches: &[build::Bench], expected: Option<u64>) -> Self {
        let nodes = expected.unwrap_or_else(|| most_common(benches.iter().map(|b| b.nodes)));
        let copies: Vec<BenchCopy> =
            benches.iter().map(|b| BenchCopy { nodes: b.nodes, nps: b.nps, differs: b.nodes != nodes }).collect();
        let mean_nps =
            if copies.is_empty() { 0 } else { copies.iter().map(|c| c.nps).sum::<u64>() / copies.len() as u64 };
        Self { copies, mean_nps, nodes }
    }
}

fn most_common(values: impl Iterator<Item = u64>) -> u64 {
    let mut counts: Vec<(u64, usize)> = Vec::new();
    for v in values {
        match counts.iter_mut().find(|(n, _)| *n == v) {
            Some((_, c)) => *c += 1,
            None => counts.push((v, 1)),
        }
    }
    // The first seen wins a tie.
    counts
        .iter()
        .fold(None::<(u64, usize)>, |best, &(n, c)| match best {
            Some((_, bc)) if bc >= c => best,
            _ => Some((n, c)),
        })
        .map_or(0, |(n, _)| n)
}

/// Runs `copies` of `<binary> bench` at once and returns each one's answer and the mean speed. A cancel (or the
/// timeout) kills them all.
pub async fn measure(
    binary: &Path,
    copies: u32,
    expected: Option<u64>,
    timeout: Duration,
    cancel: &CancellationToken,
) -> Result<Measurement> {
    let command = proc::Command::new(binary).arg("bench");
    let mut children = Vec::new();
    for _ in 0..copies.max(1) {
        children.push(proc::spawn(&command)?);
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
        out = tokio::time::timeout(timeout, outputs) => out.context("bench took too long")??,
        _ = cancel.cancelled() => return Err(Cancelled.into()),
    };
    let mut benches = Vec::new();
    for out in &outputs {
        benches.push(build::parse_bench(out).with_context(|| format!("no Bench line in: {}", out.trim()))?);
    }
    for child in &mut children {
        let status = child.wait().await?;
        if !status.success() {
            bail!("{} bench failed ({status})", binary.display());
        }
    }
    Ok(Measurement::from_benches(&benches, expected))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::build::Bench;

    #[test]
    fn takes_the_mean_and_flags_odd_copies() {
        let b = |nodes, nps| Bench { nodes, nps };
        let m = Measurement::from_benches(&[b(100, 400), b(100, 500), b(101, 600)], None);
        assert_eq!((m.nodes, m.mean_nps), (100, 500));
        assert_eq!(m.copies.iter().map(|c| c.differs).collect::<Vec<_>>(), vec![false, false, true]);
        assert!(!m.deterministic());

        let m = Measurement::from_benches(&[b(100, 400), b(100, 500)], Some(100));
        assert!(m.deterministic());
        let m = Measurement::from_benches(&[b(100, 400), b(100, 500)], Some(99));
        assert!(m.copies.iter().all(|c| c.differs));
    }

    #[test]
    fn picks_the_most_common_count() {
        assert_eq!(most_common([5, 7, 7, 5, 7].into_iter()), 7);
        assert_eq!(most_common([5, 7].into_iter()), 5);
        assert_eq!(most_common(std::iter::empty()), 0);
    }
}
