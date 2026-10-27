//! The trainer's tools, built on demand: `data` (datasets, datagen, conversion, manifests) and `bullet-train` (training
//! on the GPU), from the repository's `trainer/` workspace (its own Cargo workspace, so the app never needs CUDA to
//! compile).
//!
//! [`TrainerBuild`] runs `cargo build --release -p data` (always) and `-p bullet` (only with the CUDA Toolkit:
//! `CUDA_PATH` set, its `bin\x64` and `bin` added to `PATH` for the build and for runs) with its target directory in the
//! data directory's `tools/trainer/target`, and copies the binaries to `tools/trainer/<tree>/`, where `<tree>` is the
//! git tree hash of the trainer's sources: a commit that does not touch `trainer/` reuses them. With uncommitted
//! changes in `trainer/` it builds every time (cargo makes that cheap). Tests give the executors a fake
//! ([`Trainer`]), so they never compile the trainer.
//!
//! [`Running`] runs one of them, stdout and stderr kept apart (bullet's protocol is JSON lines on stdout; `data`
//! prints progress lines on stderr and one JSON report on stdout).

use std::{
    ffi::OsString,
    path::{Path, PathBuf},
    process::ExitStatus,
};

use anyhow::{Context, Result, bail};
use async_trait::async_trait;
use dameless_engine::proc;
use tokio_util::sync::CancellationToken;

/// A tool ready to run: its program and the environment it needs (CUDA's DLLs on `PATH` for bullet).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Tool {
    pub program: PathBuf,
    pub env: Vec<(OsString, OsString)>,
}

impl Tool {
    pub fn new(program: impl Into<PathBuf>) -> Self {
        Self { program: program.into(), env: Vec::new() }
    }

    /// A command running the tool with these arguments.
    pub fn command<I, S>(&self, args: I) -> proc::Command
    where
        I: IntoIterator<Item = S>,
        S: Into<OsString>,
    {
        let mut c = proc::Command::new(&self.program).args(args);
        for (k, v) in &self.env {
            c = c.env(k.clone(), v.clone());
        }
        c
    }
}

/// The trainer's tools, as work uses them.
#[async_trait]
pub trait Trainer: Send + Sync {
    /// The `data` tool, built first when needed.
    async fn data(&self, cancel: &CancellationToken, log: &mut (dyn for<'s> FnMut(&'s str) + Send)) -> Result<Tool>;

    /// The `bullet-train` tool, built first when needed. An error says why it cannot be (no CUDA Toolkit).
    async fn bullet(&self, cancel: &CancellationToken, log: &mut (dyn for<'s> FnMut(&'s str) + Send)) -> Result<Tool>;
}

/// The real tools, built from `trainer/`.
pub struct TrainerBuild {
    /// The trainer workspace (`<repo>/trainer`).
    source: PathBuf,
    /// `tools/trainer` in the data directory.
    out: PathBuf,
    /// Builds one at a time.
    lock: tokio::sync::Mutex<()>,
}

/// What `CUDA_PATH` says, when it names a directory.
pub fn cuda_path() -> Option<PathBuf> {
    let p = PathBuf::from(std::env::var_os("CUDA_PATH")?);
    p.is_dir().then_some(p)
}

/// `PATH` with CUDA's `bin\x64` (13.x) and `bin` in front.
fn cuda_env(cuda: &Path) -> Vec<(OsString, OsString)> {
    let mut paths = vec![cuda.join("bin").join("x64"), cuda.join("bin")];
    if let Some(path) = std::env::var_os("PATH") {
        paths.extend(std::env::split_paths(&path));
    }
    let path = std::env::join_paths(paths).unwrap_or_default();
    vec![("PATH".into(), path), ("CUDA_PATH".into(), cuda.as_os_str().to_owned())]
}

impl TrainerBuild {
    /// The tools of the trainer workspace at `source`, built under `out` (`hub.config.trainer_tools_dir()`).
    pub fn new(source: impl Into<PathBuf>, out: impl Into<PathBuf>) -> Self {
        Self { source: source.into(), out: out.into(), lock: tokio::sync::Mutex::new(()) }
    }

    /// Where the trainer's sources are: `DAMELESS_TRAINER_DIR`, else `trainer/` of the repository this was built from.
    pub fn default_source() -> PathBuf {
        match std::env::var_os("DAMELESS_TRAINER_DIR").filter(|v| !v.is_empty()) {
            Some(dir) => PathBuf::from(dir),
            None => Path::new(env!("CARGO_MANIFEST_DIR")).join("..").join("..").join("trainer"),
        }
    }

    /// The sources' tree hash, or `None` when `trainer/` has uncommitted changes (or is not in git).
    async fn tree(&self) -> Option<String> {
        let git = |args: &[&str]| proc::Command::new("git").arg("-C").arg(&self.source).args(args.iter().copied());
        let status = proc::output(&git(&["status", "--porcelain", "--", "."])).await.ok()?;
        if !status.trim().is_empty() {
            return None;
        }
        let tree = proc::output(&git(&["rev-parse", "HEAD:./"])).await.ok()?;
        Some(tree.trim().to_owned()).filter(|t| !t.is_empty())
    }

    async fn build(
        &self,
        package: &str,
        bin: &str,
        env: &[(OsString, OsString)],
        cancel: &CancellationToken,
        log: &mut (dyn for<'s> FnMut(&'s str) + Send),
    ) -> Result<Tool> {
        let _one = self.lock.lock().await;
        let tree = self.tree().await;
        let dir = self.out.join(tree.as_deref().unwrap_or("dirty"));
        let binary = dir.join(dameless_engine::exe(bin));
        if tree.is_some() && binary.is_file() {
            return Ok(Tool { program: binary, env: env.to_vec() });
        }
        if !self.source.join("Cargo.toml").is_file() {
            bail!("the trainer's sources are not at {} (set DAMELESS_TRAINER_DIR)", self.source.display());
        }
        let target = self.out.join("target");
        let mut c = proc::Command::new("cargo")
            .args(["build", "--release", "-p", package])
            .cwd(&self.source)
            .env("CARGO_TARGET_DIR", target.as_os_str());
        for var in ["RUSTUP_TOOLCHAIN", "CARGO_ENCODED_RUSTFLAGS", "CARGO_BUILD_TARGET_DIR", "CARGO_BUILD_RUSTFLAGS"] {
            c = c.env_remove(var);
        }
        for (k, v) in env {
            c = c.env(k.clone(), v.clone());
        }
        log(&c.display());
        let status = proc::run_logged(&c, cancel, &mut |l| log(l)).await?;
        if !status.success() {
            bail!("building the trainer's {package} failed ({status})");
        }
        std::fs::create_dir_all(&dir)?;
        let built = target.join("release").join(dameless_engine::exe(bin));
        std::fs::copy(&built, &binary).with_context(|| format!("copy {} to {}", built.display(), binary.display()))?;
        log(&format!("built {}", binary.display()));
        Ok(Tool { program: binary, env: env.to_vec() })
    }
}

#[async_trait]
impl Trainer for TrainerBuild {
    async fn data(&self, cancel: &CancellationToken, log: &mut (dyn for<'s> FnMut(&'s str) + Send)) -> Result<Tool> {
        self.build("data", "data", &[], cancel, log).await
    }

    async fn bullet(&self, cancel: &CancellationToken, log: &mut (dyn for<'s> FnMut(&'s str) + Send)) -> Result<Tool> {
        let Some(cuda) = cuda_path() else {
            bail!("training needs the CUDA Toolkit: install it and set CUDA_PATH (bullet compiles only with CUDA)");
        };
        self.build("bullet", "bullet-train", &cuda_env(&cuda), cancel, log).await
    }
}

/// A line a tool wrote.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Line {
    Stdout(String),
    Stderr(String),
}

/// A running tool: its lines as they come, stdout and stderr kept apart. Dropping it kills the process tree (datagen
/// and bullet are both safe to kill: they resume).
pub struct Running {
    child: proc::Child,
    lines: tokio::sync::mpsc::UnboundedReceiver<Line>,
}

impl Running {
    pub fn start(command: &proc::Command) -> Result<Self> {
        use tokio::io::{AsyncBufReadExt, BufReader};

        let mut child = proc::spawn(command)?;
        let (tx, lines) = tokio::sync::mpsc::unbounded_channel::<Line>();
        if let Some(out) = child.stdout().take() {
            let tx = tx.clone();
            tokio::spawn(async move {
                let mut lines = BufReader::new(out).lines();
                while let Ok(Some(l)) = lines.next_line().await {
                    if tx.send(Line::Stdout(l)).is_err() {
                        break;
                    }
                }
            });
        }
        if let Some(err) = child.stderr().take() {
            tokio::spawn(async move {
                let mut lines = BufReader::new(err).lines();
                while let Ok(Some(l)) = lines.next_line().await {
                    if tx.send(Line::Stderr(l)).is_err() {
                        break;
                    }
                }
            });
        }
        Ok(Self { child, lines })
    }

    /// The next line; `None` once both streams have ended.
    pub async fn next_line(&mut self) -> Option<Line> {
        self.lines.recv().await
    }

    /// Kills the whole tree and waits for it to be gone.
    pub async fn kill(&mut self) {
        let _ = self.child.kill_tree().await;
    }

    /// Waits for the process to exit.
    pub async fn wait(&mut self) -> Result<ExitStatus> {
        Ok(self.child.wait().await?)
    }
}
