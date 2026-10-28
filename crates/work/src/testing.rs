//! What the tests share: a hub in a temporary data directory over a temporary engine repository, and a fake toolchain
//! that "builds" a commit by writing its node count into a file, benches by reading it back, and plays scripted
//! fastchess rounds that write a real PGN.

use std::{
    future::Future,
    path::Path,
    process::Command,
    sync::{
        Arc, Mutex,
        atomic::{AtomicUsize, Ordering},
    },
    time::Duration,
};

use anyhow::{Result, bail};
use async_trait::async_trait;
use dameless_engine::{
    bench::Measurement,
    build::{Bench, BuildSpec, Built, PGO, PgoMissing, PgoTools},
    fastchess::{Progress, Round, RoundResult, Totals},
    tools::Installed,
};
use dameless_hub::{Config, Hub};
use tokio_util::sync::CancellationToken;

use crate::toolchain::Toolchain;

/// A temporary engine repository: the user's checkout, `main` checked out.
pub struct TestRepo {
    pub dir: tempfile::TempDir,
}

impl TestRepo {
    pub fn path(&self) -> &Path {
        self.dir.path()
    }

    pub fn git(&self, args: &[&str]) -> String {
        git(self.path(), args)
    }

    /// Commits `files` on `branch` (made from `from` when new), with the `Bench:` line the fake toolchain will measure,
    /// or `bench` when given (a stale line). Returns the sha.
    pub fn commit(&self, branch: &str, from: &str, files: &[(&str, &str)], bench: Option<u64>) -> String {
        if self.git(&["branch", "--list", branch]).is_empty() {
            self.git(&["branch", branch, from]);
        }
        let current = self.git(&["symbolic-ref", "--short", "HEAD"]);
        self.git(&["checkout", "-q", branch]);
        for (path, contents) in files {
            let p = self.path().join(path);
            std::fs::create_dir_all(p.parent().unwrap()).unwrap();
            std::fs::write(p, contents).unwrap();
            self.git(&["add", path]);
        }
        let nodes = bench.unwrap_or_else(|| fake_nodes(&self.tree_text()));
        self.git(&["commit", "-q", "-m", &format!("{branch}: change\n\nBench: {nodes}")]);
        let sha = self.git(&["rev-parse", "HEAD"]);
        self.git(&["checkout", "-q", &current]);
        sha
    }

    /// Commits `files` on `branch` (made from `from` when new) with a message that has no `Bench:` line. Returns the
    /// sha.
    pub fn commit_without_bench(&self, branch: &str, from: &str, files: &[(&str, &str)]) -> String {
        if self.git(&["branch", "--list", branch]).is_empty() {
            self.git(&["branch", branch, from]);
        }
        let current = self.git(&["symbolic-ref", "--short", "HEAD"]);
        self.git(&["checkout", "-q", branch]);
        for (path, contents) in files {
            let p = self.path().join(path);
            std::fs::create_dir_all(p.parent().unwrap()).unwrap();
            std::fs::write(p, contents).unwrap();
            self.git(&["add", path]);
        }
        self.git(&["commit", "-q", "-m", &format!("{branch}: change\n\nWhy it helps.")]);
        let sha = self.git(&["rev-parse", "HEAD"]);
        self.git(&["checkout", "-q", &current]);
        sha
    }

    /// The files the fake bench depends on, as checked out now.
    fn tree_text(&self) -> String {
        FINGERPRINTED
            .iter()
            .map(|f| std::fs::read_to_string(self.path().join(f)).unwrap_or_default())
            .collect::<Vec<_>>()
            .join("\n")
    }
}

/// The files whose contents decide the fake bench.
const FINGERPRINTED: [&str; 2] = ["src/search.rs", "src/parameters.rs"];

/// The tunables of the test repository, in Reckless's `define!` style.
pub const PARAMETERS: &str = "define! {\n    i32 rfp_margin: 80, 40, 160, 8;\n    i32 lmr_base: 100, 50, 150, 5;\n}\n";

/// What the fake build's `spsa` prints for [`PARAMETERS`].
pub const SPSA: &str = "rfp_margin, int, 80, 40, 160, 8, 0.002\nlmr_base, int, 100, 50, 150, 5, 0.002\n";

/// The node count the fake toolchain measures for some source: deterministic, and different for any change.
pub fn fake_nodes(text: &str) -> u64 {
    let mut h: u64 = 0xcbf29ce484222325;
    for b in text.bytes() {
        h ^= u64::from(b);
        h = h.wrapping_mul(0x100000001b3);
    }
    1_000_000 + h % 1_000_000
}

pub fn git(dir: &Path, args: &[&str]) -> String {
    let out = Command::new("git").arg("-C").arg(dir).args(args).output().expect("git runs");
    assert!(out.status.success(), "git {args:?}: {}", String::from_utf8_lossy(&out.stderr));
    String::from_utf8_lossy(&out.stdout).trim().to_owned()
}

/// A repository with `main` at one commit (with a correct `Bench:` line).
pub fn repo() -> TestRepo {
    let dir = tempfile::tempdir().unwrap();
    let r = TestRepo { dir };
    r.git(&["init", "-q", "-b", "main"]);
    r.git(&["config", "user.name", "Test"]);
    r.git(&["config", "user.email", "test@example.com"]);
    r.git(&["config", "core.autocrlf", "false"]);
    for (path, contents) in [("src/search.rs", "fn search() {}\n"), ("src/parameters.rs", PARAMETERS)] {
        let p = r.path().join(path);
        std::fs::create_dir_all(p.parent().unwrap()).unwrap();
        std::fs::write(p, contents).unwrap();
    }
    r.git(&["add", "."]);
    let nodes = fake_nodes(&r.tree_text());
    r.git(&["commit", "-q", "-m", &format!("Start\n\nBench: {nodes}")]);
    r
}

/// A hub in a fresh data directory over `repo`, with `cores` cores.
pub fn hub(repo: &TestRepo, cores: u32) -> (Arc<Hub>, tempfile::TempDir) {
    let dir = tempfile::tempdir().unwrap();
    let config = Config::new(dir.path().join("data"), "127.0.0.1:0".parse().unwrap(), repo.path(), cores);
    (Hub::open(config).unwrap(), dir)
}

/// A hub with no engine repository behind it, for scheduler tests.
pub fn bare_hub(cores: u32) -> (Arc<Hub>, tempfile::TempDir) {
    let dir = tempfile::tempdir().unwrap();
    let config = Config::new(dir.path().join("data"), "127.0.0.1:0".parse().unwrap(), dir.path().join("none"), cores);
    (Hub::open(config).unwrap(), dir)
}

/// Polls `check` until it holds, failing after ten seconds.
pub async fn eventually<F, Fut>(what: &str, mut check: F)
where
    F: FnMut() -> Fut,
    Fut: Future<Output = bool>,
{
    for _ in 0..1000 {
        if check().await {
            return;
        }
        tokio::time::sleep(Duration::from_millis(10)).await;
    }
    panic!("timed out waiting for {what}");
}

/// A round the fake played: what it was asked.
#[derive(Clone, Debug)]
pub struct Played {
    pub pairs: u32,
    pub concurrency: u32,
    pub dev_options: Vec<(String, String)>,
}

/// The fake toolchain.
#[derive(Default)]
pub struct Fake {
    pub builds: AtomicUsize,
    /// What every round plays, as weights of `[LL, LD, DD, WD, WW]` spread over the pairs asked.
    default_round: Mutex<Option<[u32; 5]>>,
    played: Mutex<Vec<Played>>,
    /// Rounds take this long, so a test can stop one midway.
    pub round_delay: Mutex<Duration>,
    pub build_delay: Mutex<Duration>,
    pub missing_tools: std::sync::atomic::AtomicBool,
    /// Rounds report this many times the pairs asked (the PGN still holds the pairs asked), so an SPRT between
    /// near-identical engines decides in a few rounds. Zero counts as one.
    pub pair_scale: std::sync::atomic::AtomicU32,
    /// The installed fastchess's file name, when not the default `fastchess` (a test of a tool upgrade).
    pub fastchess_name: Mutex<Option<String>>,
    /// What rounds at a node limit play, when not what [`Fake::rounds`] set (a harness check's determinism part).
    pub node_rounds: Mutex<Option<[u32; 5]>>,
    /// Writes a round's PGN (the round, the pairs that finished) instead of [`sample_pgn`].
    #[allow(clippy::type_complexity)]
    pub pgn: Mutex<Option<Box<dyn Fn(&Round, u32) -> String + Send + Sync>>>,
    /// cargo-pgo and llvm-profdata are "installed": PGO builds are made (and bench as the plain ones).
    pub pgo: std::sync::atomic::AtomicBool,
    /// PGO builds made.
    pub pgo_builds: AtomicUsize,
}

impl Fake {
    pub fn new() -> Arc<Self> {
        Arc::new(Self::default())
    }

    /// What rounds play from now on: `[0,0,0,0,1]` wins every pair, `[0,0,1,0,0]` draws them.
    pub fn rounds(&self, weights: [u32; 5]) {
        *self.default_round.lock().unwrap() = Some(weights);
    }

    pub fn played(&self) -> Vec<Played> {
        self.played.lock().unwrap().clone()
    }
}

/// Spreads `pairs` over the weights: whole shares first, what is left to the heaviest outcome.
fn spread(pairs: u32, w: [u32; 5]) -> [u32; 5] {
    let total: u32 = w.iter().sum::<u32>().max(1);
    let mut out = w.map(|x| x * pairs / total);
    let left = pairs - out.iter().sum::<u32>();
    let heaviest = (0..5).max_by_key(|i| w[*i]).unwrap_or(2);
    out[heaviest] += left;
    out
}

/// A PGN of `pairs` game pairs: in each, dev plays White then Black, and White mates.
pub fn sample_pgn(pairs: u32, dev: &str, base: &str) -> String {
    let mut out = String::new();
    for i in 0..pairs * 2 {
        let (w, b) = if i % 2 == 0 { (dev, base) } else { (base, dev) };
        out.push_str(&format!(
            "[Event \"test\"]\n[White \"{w}\"]\n[Black \"{b}\"]\n[Result \"1-0\"]\n[TimeControl \"8+0.08\"]\n\n\
             1. e4 {{+0.20/10 0.100s}} e5 2. Bc4 Nc6 3. Qh5 Nf6 4. Qxf7# 1-0\n\n"
        ));
    }
    out
}

#[async_trait]
impl Toolchain for Fake {
    async fn build(
        &self,
        spec: &BuildSpec,
        cancel: &CancellationToken,
        log: &mut (dyn for<'s> FnMut(&'s str) + Send),
    ) -> Result<Built> {
        self.builds.fetch_add(1, Ordering::SeqCst);
        let delay = *self.build_delay.lock().unwrap();
        tokio::select! {
            _ = tokio::time::sleep(delay) => {}
            _ = cancel.cancelled() => return Err(dameless_engine::Cancelled.into()),
        }
        let sha = git(&spec.repo, &["rev-parse", &format!("{}^{{commit}}", spec.commit)]);
        let text = FINGERPRINTED
            .iter()
            .map(|f| {
                Command::new("git")
                    .arg("-C")
                    .arg(&spec.repo)
                    .args(["show", &format!("{sha}:{f}")])
                    .output()
                    .ok()
                    .filter(|o| o.status.success())
                    .map(|o| String::from_utf8_lossy(&o.stdout).into_owned())
                    .unwrap_or_default()
            })
            .collect::<Vec<_>>()
            .join("\n");
        // A build with another net searches differently: the net's bytes go into the fake bench.
        let text = match &spec.evalfile {
            Some(net) => format!("{text}\n{}", String::from_utf8_lossy(&std::fs::read(net)?)),
            None => text,
        };
        std::fs::create_dir_all(&spec.out_dir)?;
        let binary = spec.out_dir.join(dameless_engine::exe("reckless"));
        let spsa = spec.features.iter().any(|f| f == "spsa");
        std::fs::write(&binary, format!("{}\n{}", fake_nodes(&text), if spsa { SPSA } else { "" }))?;
        log("fake build");
        let mut features = spec.features.clone();
        if spec.pgo {
            self.pgo_builds.fetch_add(1, Ordering::SeqCst);
            features.push(PGO.to_owned());
        }
        Ok(Built { binary, commit: sha.clone(), tree: sha, features, network: None, log: String::new() })
    }

    async fn bench(&self, binary: &Path) -> Result<Bench> {
        let text = std::fs::read_to_string(binary)?;
        let nodes = text.lines().next().unwrap_or_default().parse()?;
        Ok(Bench { nodes, nps: 1_000_000 })
    }

    async fn measure(
        &self,
        binary: &Path,
        copies: u32,
        expected: Option<u64>,
        _cancel: &CancellationToken,
    ) -> Result<Measurement> {
        let b = self.bench(binary).await?;
        // At the reference speed, so clocks are played as written.
        let benches: Vec<Bench> = (0..copies.max(1)).map(|_| Bench { nodes: b.nodes, nps: 660_000 }).collect();
        Ok(Measurement::from_benches(&benches, expected))
    }

    async fn spsa(&self, binary: &Path) -> Result<String> {
        let text = std::fs::read_to_string(binary)?;
        Ok(text.lines().skip(1).collect::<Vec<_>>().join("\n"))
    }

    async fn round(
        &self,
        round: &Round,
        cancel: &CancellationToken,
        wind_down: &CancellationToken,
        on_progress: &mut (dyn FnMut(Progress) + Send),
        _log: &mut (dyn for<'s> FnMut(&'s str) + Send),
    ) -> Result<RoundResult> {
        let delay = *self.round_delay.lock().unwrap();
        // Wound down midway, half the pairs had finished.
        let finished = tokio::select! {
            _ = tokio::time::sleep(delay) => round.pairs,
            _ = cancel.cancelled() => return Err(dameless_engine::Cancelled.into()),
            _ = wind_down.cancelled() => round.pairs / 2,
        };
        let ended_early = finished < round.pairs;
        let at_nodes = matches!(round.each.limit, dameless_engine::fastchess::Limit::Nodes { .. });
        let node_weights = if at_nodes { *self.node_rounds.lock().unwrap() } else { None };
        let Some(weights) = node_weights.or(*self.default_round.lock().unwrap()) else { bail!("no rounds set") };
        let scale = self.pair_scale.load(Ordering::SeqCst).max(1);
        let penta = spread(finished * scale, weights);
        let [ll, ld, dd, wd, ww] = penta;
        let totals = Totals { penta, wins: wd + 2 * ww, draws: ld + 2 * dd + wd, losses: 2 * ll + ld };
        self.played.lock().unwrap().push(Played {
            pairs: round.pairs,
            concurrency: round.concurrency,
            dev_options: round.dev.options.clone(),
        });
        if let Some(dir) = round.pgn.parent() {
            std::fs::create_dir_all(dir)?;
        }
        let pgn = match &*self.pgn.lock().unwrap() {
            Some(make) => make(round, finished),
            None => sample_pgn(finished, &round.dev.name, &round.base.name),
        };
        std::fs::write(&round.pgn, pgn)?;
        on_progress(Progress::Totals(totals));
        Ok(RoundResult { totals, pgn: round.pgn.clone(), ended_early })
    }

    async fn pgo(&self) -> Result<PgoTools, PgoMissing> {
        if self.pgo.load(Ordering::SeqCst) {
            Ok(PgoTools { cargo_pgo: "cargo-pgo 0.3.0".into(), llvm_profdata: "llvm-profdata".into() })
        } else {
            Err(PgoMissing::CargoPgo)
        }
    }

    fn tools(&self, dir: &Path) -> Installed {
        if self.missing_tools.load(Ordering::SeqCst) {
            Installed { fastchess: None, book: None }
        } else {
            let name = self.fastchess_name.lock().unwrap().clone().unwrap_or_else(|| "fastchess".into());
            Installed { fastchess: Some(dir.join(name)), book: Some(dir.join("book.epd")) }
        }
    }
}

/// A machine for health and harness tests: whatever the test sets, and a count of the slow probes asked.
pub struct FakeProbes {
    pub machine: Mutex<dameless_core::harness::Machine>,
    pub clang: Mutex<Option<String>>,
    pub cuda: Mutex<crate::health::Cuda>,
    pub gpu: Mutex<Option<crate::health::Gpu>>,
    pub disk: Mutex<Option<crate::health::DiskSpace>>,
    pub slow_calls: AtomicUsize,
}

impl FakeProbes {
    /// A healthy machine: clang, CUDA, a GPU and 500 GB free.
    pub fn new() -> Arc<Self> {
        Arc::new(Self {
            machine: Mutex::new(dameless_core::harness::Machine {
                cpu: "Test CPU @ 3.80GHz".into(),
                cores: 8,
                os: "Windows 11".into(),
            }),
            clang: Mutex::new(Some("clang version 18.1.8".into())),
            cuda: Mutex::new(crate::health::Cuda {
                path: Some("C:/CUDA/v12.4".into()),
                nvcc: Some("Cuda compilation tools, release 12.4, V12.4.131".into()),
            }),
            gpu: Mutex::new(Some(crate::health::Gpu {
                name: "NVIDIA GeForce RTX 3080".into(),
                memory_mb: Some(10240),
            })),
            disk: Mutex::new(Some(crate::health::DiskSpace {
                free_bytes: 500_000_000_000,
                total_bytes: 1_000_000_000_000,
            })),
            slow_calls: AtomicUsize::new(0),
        })
    }
}

#[async_trait]
impl crate::health::Probes for FakeProbes {
    fn machine(&self) -> dameless_core::harness::Machine {
        self.machine.lock().unwrap().clone()
    }

    async fn clang(&self) -> Option<String> {
        self.slow_calls.fetch_add(1, Ordering::SeqCst);
        self.clang.lock().unwrap().clone()
    }

    async fn cuda(&self) -> crate::health::Cuda {
        self.cuda.lock().unwrap().clone()
    }

    async fn gpu(&self) -> Option<crate::health::Gpu> {
        self.gpu.lock().unwrap().clone()
    }

    fn disk(&self, _path: &Path) -> Option<crate::health::DiskSpace> {
        *self.disk.lock().unwrap()
    }
}
