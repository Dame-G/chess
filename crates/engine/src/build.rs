//! Building a commit of the engine into a binary, and the commands a built binary answers: `bench` (its node count
//! and speed) and `spsa` (its tunables, raw).
//!
//! A build checks the commit out in a worktree under the caller's directory and runs cargo there, as Reckless's
//! Makefile does (`cargo rustc --release` with `-C target-cpu=native`). Tablebase support (the default `syzygy` feature)
//! compiles C with clang; on a machine without clang (Windows, usually) the build is made with
//! `--no-default-features`, which is Reckless's `make no-syzygy`. Which one was made is in [`Built::features`].
//!
//! Caching builds (by tree and features) and recording them is the caller's job: this module only builds.

use std::path::{Path, PathBuf};

use anyhow::{Context, Result, bail};
use serde::{Deserialize, Serialize};
use tokio_util::sync::CancellationToken;

use crate::{exe, git::Repo, proc};

/// What to build, and where.
#[derive(Clone, Debug)]
pub struct BuildSpec {
    /// The fork's checkout. Only read from: the build happens in `worktree`.
    pub repo: PathBuf,
    /// The commit to build (a sha, or any ref).
    pub commit: String,
    /// Cargo features on top of the default ones, such as `spsa` (tunables as UCI options).
    pub features: Vec<String>,
    /// Tablebase support: `Some` forces it on or off, `None` turns it on when clang is found.
    pub syzygy: Option<bool>,
    /// Compile for this machine's CPU (`-C target-cpu=native`), as Reckless's Makefile does. Turn it off for a binary
    /// meant to run elsewhere.
    pub native: bool,
    /// The build worktree, created or reused.
    pub worktree: PathBuf,
    /// Cargo's target directory, shared by builds so dependencies compile once.
    pub target_dir: PathBuf,
    /// Where the binary is copied: `<out_dir>/reckless[.exe]`.
    pub out_dir: PathBuf,
    /// A folder of network files shared by builds, so each network is downloaded once. Optional.
    pub networks_dir: Option<PathBuf>,
}

/// A finished build.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Built {
    pub binary: PathBuf,
    /// The commit built (full sha) and its tree.
    pub commit: String,
    pub tree: String,
    /// `syzygy` or `no-syzygy`, then the extra features: `["no-syzygy", "spsa"]`.
    pub features: Vec<String>,
    /// The network file the build embeds, from its build script.
    pub network: Option<String>,
    /// Everything cargo wrote.
    pub log: String,
}

impl Built {
    /// The features as one label, as the old harness stored them: `no-syzygy+spsa`.
    pub fn feature_label(&self) -> String {
        self.features.join("+")
    }
}

/// `reckless bench`'s answer.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Bench {
    /// Nodes searched: the functional fingerprint.
    pub nodes: u64,
    /// Nodes per second on this machine, as it was loaded then.
    pub nps: u64,
}

/// Builds a commit. Each line cargo writes goes to `log` as it comes (and into [`Built::log`]). A cancel kills cargo
/// and its compilers.
pub async fn build(spec: &BuildSpec, cancel: &CancellationToken, log: &mut (dyn FnMut(&str) + Send)) -> Result<Built> {
    let repo = Repo::new(&spec.repo);
    let commit = repo.resolve(&spec.commit).await?;
    let syzygy = match spec.syzygy {
        Some(s) => s,
        None => has_clang().await,
    };
    let mut features = vec![if syzygy { "syzygy" } else { "no-syzygy" }.to_owned()];
    features.extend(spec.features.iter().cloned());

    let mut transcript = String::new();
    let mut say = |line: &str| {
        transcript.push_str(line);
        transcript.push('\n');
        log(line);
    };
    say(&format!("checking out {} {}", short(&commit.sha), commit.subject));
    repo.checkout_worktree(&spec.worktree, &commit.sha).await?;

    let network = network_name(&spec.worktree);
    let mut command = cargo_command(spec, syzygy);
    if let (Some(name), Some(nets)) = (&network, &spec.networks_dir) {
        std::fs::create_dir_all(nets)?;
        let cached = nets.join(name);
        let in_checkout = spec.repo.join("networks").join(name);
        if !cached.exists() && in_checkout.exists() {
            std::fs::copy(&in_checkout, &cached)?;
        }
        if cached.exists() {
            command = command.env("EVALFILE", cached.as_os_str());
        }
    }
    say(&command.display());
    let status = proc::run_logged(&command, cancel, &mut say).await?;
    if !status.success() {
        bail!("cargo build failed ({status})");
    }

    // A network the build script downloaded is kept for the next build.
    if let (Some(name), Some(nets)) = (&network, &spec.networks_dir) {
        let downloaded = spec.worktree.join("networks").join(name);
        if !nets.join(name).exists() && downloaded.exists() {
            std::fs::copy(&downloaded, nets.join(name))?;
        }
    }

    std::fs::create_dir_all(&spec.out_dir)?;
    let binary = spec.out_dir.join(exe("reckless"));
    let built = spec.target_dir.join("release").join(exe("reckless"));
    std::fs::copy(&built, &binary).with_context(|| format!("copy {} to {}", built.display(), binary.display()))?;
    say(&format!("built {}", binary.display()));
    Ok(Built { binary, commit: commit.sha, tree: commit.tree, features, network, log: transcript })
}

/// The cargo command for a build: `cargo build --release --bin reckless`, with the features asked for, in the worktree.
fn cargo_command(spec: &BuildSpec, syzygy: bool) -> proc::Command {
    let mut c = proc::Command::new("cargo").args(["build", "--release", "--bin", "reckless"]);
    if !syzygy {
        c = c.arg("--no-default-features");
    }
    if !spec.features.is_empty() {
        c = c.args(["--features", &spec.features.join(",")]);
    }
    c = c.cwd(&spec.worktree).env("CARGO_TARGET_DIR", spec.target_dir.as_os_str());
    // When the server itself runs under cargo or rustup, these would leak into the engine's build and pick our
    // toolchain or flags instead of the checkout's.
    for var in ["RUSTUP_TOOLCHAIN", "CARGO_ENCODED_RUSTFLAGS", "CARGO_BUILD_TARGET_DIR", "CARGO_BUILD_RUSTFLAGS"] {
        c = c.env_remove(var);
    }
    if spec.native {
        c = c.env("RUSTFLAGS", "-C target-cpu=native");
    } else {
        c = c.env_remove("RUSTFLAGS");
    }
    c
}

fn short(sha: &str) -> &str {
    sha.get(..8).unwrap_or(sha)
}

/// Whether clang is on the PATH (needed for tablebase support).
pub async fn has_clang() -> bool {
    proc::output(&proc::Command::new("clang").arg("--version")).await.is_ok()
}

/// The network file a checkout embeds by default, from its build script (`const NETWORK_NAME: &str = "…";`).
pub fn network_name(worktree: &Path) -> Option<String> {
    let script = std::fs::read_to_string(worktree.join("build").join("build.rs")).ok()?;
    let line = script.lines().find(|l| l.contains("NETWORK_NAME") && l.contains('='))?;
    Some(line.split('"').nth(1)?.to_owned())
}

/// Runs `<binary> bench` and returns its node count and speed.
pub async fn bench(binary: &Path) -> Result<Bench> {
    let out = proc::output(&proc::Command::new(binary).arg("bench")).await?;
    parse_bench(&out).with_context(|| format!("no Bench line in: {}", out.trim()))
}

/// `Bench: 2837905 nodes 352413 nps`: the node count and the speed.
pub fn parse_bench(out: &str) -> Option<Bench> {
    let line = out.lines().find_map(|l| l.trim().strip_prefix("Bench:"))?;
    let words: Vec<&str> = line.split_whitespace().collect();
    Some(Bench { nodes: words.first()?.parse().ok()?, nps: words.get(2)?.parse().ok()? })
}

/// What `<binary> spsa` prints: an `spsa` build's tunables in OpenBench's format, one per line
/// (`name, int, default, min, max, c_end, r_end`). Parsing it is the domain's job. Empty for a build without the
/// feature.
pub async fn spsa_raw(binary: &Path) -> Result<String> {
    proc::output(&proc::Command::new(binary).arg("spsa")).await
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_bench_output() {
        assert_eq!(parse_bench("Bench: 2837905 nodes 352413 nps\n"), Some(Bench { nodes: 2837905, nps: 352413 }));
        assert_eq!(parse_bench("info string x\n  Bench: 1 nodes 2 nps"), Some(Bench { nodes: 1, nps: 2 }));
        assert_eq!(parse_bench("nothing"), None);
    }

    fn spec(features: &[&str]) -> BuildSpec {
        BuildSpec {
            repo: "repo".into(),
            commit: "main".into(),
            features: features.iter().map(|f| f.to_string()).collect(),
            syzygy: None,
            native: true,
            worktree: "wt".into(),
            target_dir: "target".into(),
            out_dir: "out".into(),
            networks_dir: None,
        }
    }

    #[test]
    fn builds_the_cargo_command() {
        let c = cargo_command(&spec(&[]), true);
        assert_eq!(c.display(), "cargo build --release --bin reckless");
        let c = cargo_command(&spec(&["spsa"]), false);
        assert_eq!(c.display(), "cargo build --release --bin reckless --no-default-features --features spsa");
        assert!(c.env.iter().any(|(k, v)| k == "RUSTFLAGS" && v == "-C target-cpu=native"));
        assert!(c.env.iter().any(|(k, v)| k == "CARGO_TARGET_DIR" && v == "target"));
        assert!(c.env_remove.iter().any(|k| k == "RUSTUP_TOOLCHAIN"));
        assert_eq!(c.cwd.as_deref(), Some(Path::new("wt")));
    }

    #[test]
    fn labels_features() {
        let built = Built {
            binary: "b".into(),
            commit: "c".into(),
            tree: "t".into(),
            features: vec!["no-syzygy".into(), "spsa".into()],
            network: None,
            log: String::new(),
        };
        assert_eq!(built.feature_label(), "no-syzygy+spsa");
    }

    #[test]
    fn reads_the_network_name() {
        let dir = crate::testing::TempDir::new("network");
        std::fs::create_dir_all(dir.path().join("build")).unwrap();
        std::fs::write(dir.path().join("build/build.rs"), "const NETWORK_NAME: &str = \"v60-7f587dfb.nnue\";\n")
            .unwrap();
        assert_eq!(network_name(dir.path()).as_deref(), Some("v60-7f587dfb.nnue"));
        assert_eq!(network_name(Path::new("nowhere")), None);
    }

    /// A real build of the user's checkout. Slow (minutes); run with
    /// `RECKLESS_DIR=… cargo test -p dameless-engine -- --ignored builds_reckless`. The checkout is only read: it is
    /// cloned (`--shared`) into a temp directory and built there, its network file copied into the networks folder.
    #[tokio::test]
    #[ignore = "builds Reckless: slow, needs RECKLESS_DIR"]
    async fn builds_reckless() {
        let original = PathBuf::from(std::env::var("RECKLESS_DIR").expect("RECKLESS_DIR"));
        let dir = crate::testing::TempDir::new("build");
        let repo = dir.path().join("clone");
        let (from, to) = (original.to_str().unwrap(), repo.to_str().unwrap());
        proc::output(&proc::Command::new("git").args(["clone", "-q", "--shared", from, to])).await.unwrap();
        let networks = dir.path().join("networks");
        std::fs::create_dir_all(&networks).unwrap();
        for entry in std::fs::read_dir(original.join("networks")).into_iter().flatten().flatten() {
            std::fs::copy(entry.path(), networks.join(entry.file_name())).unwrap();
        }
        let spec = BuildSpec {
            repo: repo.clone(),
            commit: "main".into(),
            features: vec![],
            syzygy: None,
            native: true,
            worktree: dir.path().join("worktree"),
            target_dir: dir.path().join("target"),
            out_dir: dir.path().join("out"),
            networks_dir: Some(networks),
        };
        let built = build(&spec, &CancellationToken::new(), &mut |l| println!("{l}")).await.unwrap();
        let b = bench(&built.binary).await.unwrap();
        let expected = Repo::new(&repo).resolve("main").await.unwrap().bench;
        assert_eq!(Some(b.nodes), expected);
    }
}
