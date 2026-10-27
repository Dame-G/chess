//! The trainer's tools, faked for tests: one small program that speaks both `data`'s and `bullet-train`'s protocols,
//! compiled with rustc on first use (std only, about a second), and [`FakeTrainer`], which hands it to the executors.
//!
//! - `datagen` appends 4 bytes per position to `part-000.vf` (so a rerun resumes), writes a progress JSON line on
//!   stderr per step, the manifest at the end, and its report on stdout;
//! - `manifest` sums the directory's `.vf` files (4 bytes a position);
//! - `convert` writes its inputs twice over to `-o`;
//! - `train` prints bullet's JSON lines, writes checkpoints (`--save-rate`), resumes after `--resume <dir>`, and writes
//!   a net whose bytes depend on its name.
//!
//! Every run appends its arguments to `args.log` in its output directory. `FAKE_SLEEP_MS` slows each step (so a test
//! can stop it midway), `FAKE_TRAIN_FAIL` makes `train` fail with bullet's `error` line.

use std::{
    path::PathBuf,
    process::Command,
    sync::{Arc, Mutex, OnceLock, atomic::AtomicBool, atomic::Ordering},
};

use anyhow::{Result, bail};
use async_trait::async_trait;
use tokio_util::sync::CancellationToken;

use crate::trainer::{Tool, Trainer};

pub const FAKE_TOOL: &str = r##"
use std::{env, fs, io::Write, path::Path, thread, time::Duration};

fn arg(a: &[String], name: &str) -> Option<String> {
    a.iter().position(|x| x == name).and_then(|i| a.get(i + 1).cloned())
}

fn nap() {
    let ms: u64 = env::var("FAKE_SLEEP_MS").ok().and_then(|v| v.parse().ok()).unwrap_or(0);
    thread::sleep(Duration::from_millis(ms));
}

fn esc(s: &str) -> String {
    s.replace('\\', "\\\\").replace('"', "\\\"")
}

fn log_args(dir: &Path, a: &[String]) {
    fs::create_dir_all(dir).unwrap();
    let mut f = fs::OpenOptions::new().create(true).append(true).open(dir.join("args.log")).unwrap();
    writeln!(f, "{}", a[1..].join(" ")).unwrap();
}

fn manifest(dir: &Path, name: &str, source: &str, units: Option<String>) {
    let mut files = Vec::new();
    let (mut total, mut size) = (0u64, 0u64);
    let mut entries: Vec<_> = fs::read_dir(dir).unwrap().flatten().map(|e| e.path()).collect();
    entries.sort();
    for p in entries {
        if p.extension().map_or(false, |x| x == "vf") {
            let len = fs::metadata(&p).unwrap().len();
            total += len / 4;
            size += len;
            files.push(format!(
                "{{\"path\":\"{}\",\"format\":\"viriformat\",\"size_bytes\":{},\"positions\":{},\"games\":{},\"sha256\":\"x\"}}",
                esc(&p.file_name().unwrap().to_string_lossy()), len, len / 4, len / 400
            ));
        }
    }
    let units = units.map(|u| format!("\"{}\"", u)).unwrap_or_else(|| "null".into());
    let text = format!(
        "{{\"name\":\"{}\",\"format\":\"viriformat\",\"positions\":{},\"games\":{},\"size_bytes\":{},\"source\":\"{}\",\"score_units\":{},\"generator\":{{\"tool\":\"fake\"}},\"created\":\"2026-10-26T00:00:00Z\",\"updated\":\"2026-10-26T00:00:00Z\",\"files\":[{}]}}",
        esc(name), total, total / 100, size, esc(source), units, files.join(",")
    );
    fs::write(dir.join("manifest.json"), text).unwrap();
}

fn main() {
    let a: Vec<String> = env::args().collect();
    match a.get(1).map(String::as_str) {
        Some("datagen") => {
            let out = Path::new(arg(&a, "--out").as_deref().unwrap()).to_owned();
            log_args(&out, &a);
            let target: u64 = arg(&a, "--positions").unwrap().parse().unwrap();
            let max: u64 = arg(&a, "--max-size").unwrap().parse().unwrap();
            let part = out.join("part-000.vf");
            let mut have = fs::metadata(&part).map(|m| m.len()).unwrap_or(0) / 4;
            let step = (target / 20).max(1);
            let mut finished = "target";
            while have < target {
                let n = step.min(target - have);
                if (have + n) * 4 > max {
                    finished = "disk-cap";
                    break;
                }
                let mut f = fs::OpenOptions::new().create(true).append(true).open(&part).unwrap();
                f.write_all(&vec![7u8; (n * 4) as usize]).unwrap();
                have += n;
                eprintln!(
                    "{{\"positions\":{},\"games\":{},\"bytes\":{},\"target_positions\":{},\"max_bytes\":{},\"session_positions\":{},\"positions_per_second\":1000.0,\"elapsed_seconds\":1.0}}",
                    have, have / 100, have * 4, target, max, n
                );
                nap();
            }
            manifest(&out, &out.file_name().unwrap().to_string_lossy(), "datagen", Some("reckless-internal".into()));
            println!("{{\n  \"positions\": {},\n  \"finished\": \"{}\"\n}}", have, finished);
        }
        Some("manifest") => {
            let dir = Path::new(&a[2]).to_owned();
            let name = arg(&a, "--name").unwrap_or_else(|| "x".into());
            manifest(&dir, &name, &arg(&a, "--source").unwrap_or_default(), arg(&a, "--units"));
            println!("{{}}");
        }
        Some("convert") => {
            let out = Path::new(arg(&a, "-o").as_deref().unwrap()).to_owned();
            log_args(out.parent().unwrap(), &a);
            let mut bytes = Vec::new();
            for input in a[2..].iter().take_while(|x| x.as_str() != "-o") {
                bytes.extend(fs::read(input).unwrap());
            }
            let twice = [bytes.clone(), bytes].concat();
            fs::write(&out, twice).unwrap();
            println!("{{\"games_in\": 1}}");
        }
        Some("train") => {
            let out = Path::new(arg(&a, "--out").as_deref().unwrap()).to_owned();
            log_args(&out, &a);
            let name = arg(&a, "--name").unwrap();
            let total: u32 = arg(&a, "--superbatches").unwrap().parse().unwrap();
            let bps: u32 = arg(&a, "--batches-per-superbatch").unwrap().parse().unwrap();
            let save: u32 = arg(&a, "--save-rate").unwrap().parse().unwrap();
            let start = match arg(&a, "--resume") {
                Some(r) => r.rsplit('-').next().unwrap().parse::<u32>().unwrap() + 1,
                None => 1,
            };
            println!("{{\"type\":\"started\",\"device\":\"fake\",\"start_superbatch\":{},\"superbatches\":{},\"batch_size\":1,\"batches_per_superbatch\":{},\"parent\":null,\"resumed_from\":null,\"bullet\":\"fake\"}}", start, total, bps);
            if env::var("FAKE_TRAIN_FAIL").is_ok() {
                println!("{{\"type\":\"error\",\"message\":\"no data in the files\"}}");
                std::process::exit(1);
            }
            for sb in start..=total {
                nap();
                let loss = 1.0 / sb as f64;
                println!("{{\"type\":\"progress\",\"superbatch\":{},\"batch\":{},\"loss\":{},\"lr\":0.0001,\"wdl\":0.3,\"positions_per_sec\":700000.0,\"grad_norm\":0.5,\"grad_norms\":{{}},\"positions\":1,\"elapsed_seconds\":{}.0}}", sb, bps, loss, sb);
                println!("{{\"type\":\"superbatch\",\"superbatch\":{},\"loss\":{},\"positions_per_sec\":700000.0,\"elapsed_seconds\":1.0}}", sb, loss);
                if sb % save == 0 || sb == total {
                    let dir = out.join("checkpoints").join(format!("{}-{}", name, sb));
                    fs::create_dir_all(&dir).unwrap();
                    fs::write(dir.join("state.json"), "{}").unwrap();
                    fs::write(dir.join(format!("{}-{}.nnue", name, sb)), format!("ck {} {}", name, sb)).unwrap();
                    println!("{{\"type\":\"checkpoint\",\"superbatch\":{},\"path\":\"{}\",\"net\":\"x\",\"sha256\":\"x\"}}", sb, esc(&dir.to_string_lossy()));
                }
                std::io::stdout().flush().unwrap();
            }
            let net = out.join(format!("{}.nnue", name));
            fs::write(&net, format!("fake net {}", name)).unwrap();
            println!("{{\"type\":\"done\",\"net\":\"{}\",\"sha256\":\"x\",\"superbatches\":{},\"positions\":1,\"elapsed_seconds\":1.0,\"dropped_inputs\":0}}", esc(&net.to_string_lossy()), total);
        }
        _ => {
            eprintln!("fake trainer: unknown command {:?}", a.get(1));
            std::process::exit(2);
        }
    }
}
"##;

/// The fake program, compiled once per test binary (next to it, in the target directory).
pub fn fake_tool() -> PathBuf {
    static TOOL: OnceLock<PathBuf> = OnceLock::new();
    TOOL.get_or_init(|| {
        let dir = std::env::current_exe().unwrap().parent().unwrap().join("dameless-fake-trainer");
        std::fs::create_dir_all(&dir).unwrap();
        let exe = dir.join(dameless_engine::exe("fake-trainer"));
        let src = dir.join("fake.rs");
        if exe.is_file() && std::fs::read_to_string(&src).is_ok_and(|s| s == FAKE_TOOL) {
            return exe;
        }
        let id = std::process::id();
        let tmp_src = dir.join(format!("fake-{id}.rs"));
        let tmp = dir.join(dameless_engine::exe(&format!("fake-{id}")));
        std::fs::write(&tmp_src, FAKE_TOOL).unwrap();
        let status = Command::new("rustc")
            .args(["--edition", "2021", "-C", "debuginfo=0"])
            .arg(&tmp_src)
            .arg("-o")
            .arg(&tmp)
            .status()
            .expect("rustc runs");
        assert!(status.success(), "the fake trainer did not compile");
        let _ = std::fs::remove_file(&exe);
        std::fs::rename(&tmp, &exe).unwrap();
        let _ = std::fs::rename(&tmp_src, &src);
        let _ = std::fs::remove_file(dir.join(format!("fake-{id}.pdb")));
        exe
    })
    .clone()
}

/// The trainer's tools as the fake program, with extra environment for it.
#[derive(Default)]
pub struct FakeTrainer {
    pub env: Mutex<Vec<(String, String)>>,
    /// bullet cannot be built (no CUDA).
    pub no_cuda: AtomicBool,
}

impl FakeTrainer {
    pub fn new() -> Arc<Self> {
        Arc::new(Self::default())
    }

    pub fn set_env(&self, key: &str, value: &str) {
        let mut env = self.env.lock().unwrap();
        env.retain(|(k, _)| k != key);
        env.push((key.into(), value.into()));
    }

    pub fn clear_env(&self) {
        self.env.lock().unwrap().clear();
    }

    fn tool(&self) -> Tool {
        Tool { program: fake_tool(), env: self.env.lock().unwrap().iter().map(|(k, v)| (k.into(), v.into())).collect() }
    }
}

#[async_trait]
impl Trainer for FakeTrainer {
    async fn data(&self, _cancel: &CancellationToken, _log: &mut (dyn for<'s> FnMut(&'s str) + Send)) -> Result<Tool> {
        Ok(self.tool())
    }

    async fn bullet(
        &self,
        _cancel: &CancellationToken,
        _log: &mut (dyn for<'s> FnMut(&'s str) + Send),
    ) -> Result<Tool> {
        if self.no_cuda.load(Ordering::SeqCst) {
            bail!("training needs the CUDA Toolkit");
        }
        Ok(self.tool())
    }
}
