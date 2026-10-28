//! Everything work does to the outside world of the engine, behind one trait: building a commit, benching a binary,
//! listing its tunables, playing a fastchess round, and finding the tools. [`Reckless`] does it for real with the
//! `engine` adapter; tests give the scheduler a fake, so they never compile Reckless or run fastchess.

use std::path::{Path, PathBuf};

use anyhow::Result;
use async_trait::async_trait;
use dameless_engine::{
    bench::{self, Measurement},
    build::{self, Bench, BuildSpec, Built, PgoMissing, PgoTools},
    fastchess::{self, Progress, Round, RoundResult},
    tools::{self, Installed},
};
use tokio_util::sync::CancellationToken;

/// The engine's outside world, as work uses it.
#[async_trait]
pub trait Toolchain: Send + Sync {
    /// Builds a commit (see `engine::build::build`).
    async fn build(
        &self,
        spec: &BuildSpec,
        cancel: &CancellationToken,
        log: &mut (dyn for<'s> FnMut(&'s str) + Send),
    ) -> Result<Built>;

    /// One `bench` of a binary: its node count and speed.
    async fn bench(&self, binary: &Path) -> Result<Bench>;

    /// `copies` benches at once, as clock scaling measures (see `engine::bench::measure`).
    async fn measure(
        &self,
        binary: &Path,
        copies: u32,
        expected: Option<u64>,
        cancel: &CancellationToken,
    ) -> Result<Measurement>;

    /// What `<binary> spsa` prints: an `spsa` build's tunables in OpenBench's format.
    async fn spsa(&self, binary: &Path) -> Result<String>;

    /// Plays one fastchess round to the end, or until `wind_down` fires: then it stops at once and returns the pairs
    /// that finished (`ended_early`). `cancel` stops it and returns `Cancelled`.
    async fn round(
        &self,
        round: &Round,
        cancel: &CancellationToken,
        wind_down: &CancellationToken,
        on_progress: &mut (dyn FnMut(Progress) + Send),
        log: &mut (dyn for<'s> FnMut(&'s str) + Send),
    ) -> Result<RoundResult>;

    /// fastchess and the opening book in the tools directory, if installed.
    fn tools(&self, dir: &Path) -> Installed;

    /// cargo-pgo and llvm-profdata, which PGO builds need, or what is missing. A toolchain that does not say makes
    /// no PGO builds.
    async fn pgo(&self) -> Result<PgoTools, PgoMissing> {
        Err(PgoMissing::Unsupported)
    }
}

/// The real thing: cargo, `reckless bench`, fastchess.
#[derive(Clone, Copy, Debug, Default)]
pub struct Reckless;

#[async_trait]
impl Toolchain for Reckless {
    async fn build(
        &self,
        spec: &BuildSpec,
        cancel: &CancellationToken,
        log: &mut (dyn for<'s> FnMut(&'s str) + Send),
    ) -> Result<Built> {
        build::build(spec, cancel, log).await
    }

    async fn bench(&self, binary: &Path) -> Result<Bench> {
        build::bench(binary).await
    }

    async fn measure(
        &self,
        binary: &Path,
        copies: u32,
        expected: Option<u64>,
        cancel: &CancellationToken,
    ) -> Result<Measurement> {
        bench::measure(binary, copies, expected, bench::BENCH_TIMEOUT, cancel).await
    }

    async fn spsa(&self, binary: &Path) -> Result<String> {
        build::spsa_raw(binary).await
    }

    async fn round(
        &self,
        round: &Round,
        cancel: &CancellationToken,
        wind_down: &CancellationToken,
        on_progress: &mut (dyn FnMut(Progress) + Send),
        log: &mut (dyn for<'s> FnMut(&'s str) + Send),
    ) -> Result<RoundResult> {
        fastchess::run_round(round, cancel, wind_down, on_progress, log).await
    }

    fn tools(&self, dir: &Path) -> Installed {
        tools::installed(dir)
    }

    async fn pgo(&self) -> Result<PgoTools, PgoMissing> {
        build::pgo_tools().await
    }
}

/// fastchess and the book, or the sentence that says how to get them.
pub fn require_tools(installed: &Installed) -> Result<(PathBuf, PathBuf), String> {
    match (&installed.fastchess, &installed.book) {
        (Some(f), Some(b)) => Ok((f.clone(), b.clone())),
        _ => Err("fastchess and the opening book are not installed: run `dameless setup`".to_owned()),
    }
}
