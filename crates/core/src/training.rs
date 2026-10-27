//! Training: networks, the data they are trained on, and the rules for training runs (phase R4).
//!
//! A **net** is a build input: Reckless embeds its network at compile time (`EVALFILE`), so a version of Dameless is
//! code plus a net, and a build is keyed by (commit, features, net). The shipped net of the champion is registered on
//! first run; every trained net is registered when its run ends.
//!
//! A **dataset** is a directory of training data with a `manifest.json` (written by the trainer's `data` tool): self-
//! play from datagen, an imported shard of someone else's data, or a trial's games. Datasets share one disk cap.
//!
//! A **training run** fine-tunes a net (the champion's by default) or trains one from scratch on datasets, with
//! bullet on the GPU (`trainer/`). It is work; a `net` idea runs one as its first stage and then tests the champion's
//! code with the new net through the usual funnel.
//!
//! This module is the pure part: the types, the schedule's defaults, the disk rules and the names people read.

use std::{fmt, str::FromStr};

use serde::{Deserialize, Serialize};
use utoipa::ToSchema;

use crate::ids::{IdeaId, Timestamp, WorkId};

macro_rules! row_id {
    ($(#[$doc:meta])* $name:ident) => {
        $(#[$doc])*
        #[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize, ToSchema)]
        #[serde(transparent)]
        pub struct $name(pub i64);

        impl fmt::Display for $name {
            fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                self.0.fmt(f)
            }
        }

        impl FromStr for $name {
            type Err = std::num::ParseIntError;

            fn from_str(s: &str) -> Result<Self, Self::Err> {
                s.trim().parse().map(Self)
            }
        }
    };
}

row_id!(
    /// A network file the app knows.
    NetId
);
row_id!(
    /// A dataset directory the app knows.
    DatasetId
);

/// The architecture every net here has: Reckless `main`'s (threat inputs, 10 king buckets, pairwise, 768 → 16 → 32 → 1
/// with 8 output buckets). A net of another architecture is a different size.
pub const RECKLESS_ARCH: &str = "reckless-v60";

/// The size of a [`RECKLESS_ARCH`] net file, in bytes.
pub const RECKLESS_NET_BYTES: u64 = 63_266_880;

/// A network file.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, ToSchema)]
pub struct Net {
    pub id: NetId,
    /// `v60-7f587dfb` for a shipped net, the run's name for a trained one (`net-3`).
    pub name: String,
    pub sha256: String,
    /// The file in the data directory's `nets/`, absolute.
    pub path: String,
    /// The architecture, [`RECKLESS_ARCH`] for every net so far.
    pub arch: String,
    pub size_bytes: u64,
    /// The net it was fine-tuned from.
    pub parent: Option<NetId>,
    /// The (first) dataset it was trained on.
    pub dataset: Option<DatasetId>,
    /// The run that made it; none for a shipped net.
    pub run: Option<NetRun>,
    /// Offline metrics (validation loss, ...), when measured. Not measured yet: always null for now.
    pub metrics: Option<serde_json::Value>,
    pub created_at: Timestamp,
}

/// The training run a net came from.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, ToSchema)]
pub struct NetRun {
    /// The idea it was trained for, when it was.
    pub idea: Option<IdeaId>,
    /// The `Train` work item.
    pub work: Option<WorkId>,
    pub superbatches: u32,
    /// Positions trained on (bullet's count, the dataset cycled).
    pub positions: u64,
    /// The last superbatch's mean loss.
    pub final_loss: Option<f64>,
}

/// Where a dataset came from.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, ToSchema)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum DatasetSource {
    /// Self-play with Reckless's `datagen` branch; `settings` as the tool recorded them.
    Datagen { settings: serde_json::Value },
    /// A shard downloaded (with your go-ahead) and converted.
    Import { url: String },
    /// An idea's trial games, converted.
    Trials { idea: IdeaId },
    /// Anything else the manifest names.
    Other { source: String },
}

/// A dataset: a directory with a `manifest.json`.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, ToSchema)]
pub struct Dataset {
    pub id: DatasetId,
    /// Unique: also its directory's name under the data directory's `data/`.
    pub name: String,
    /// `viriformat`, `binpack`, `mixed`, ... as the manifest says.
    pub format: String,
    pub positions: u64,
    pub games: u64,
    pub size_bytes: u64,
    pub source: DatasetSource,
    /// `cp` or `reckless_internal`; training wants Reckless's internal units.
    pub score_units: Option<String>,
    /// The directory, absolute.
    pub path: String,
    pub created_at: Timestamp,
    pub updated_at: Timestamp,
}

// ---------------------------------------------------------------------------------------------------------------------
// Training runs
// ---------------------------------------------------------------------------------------------------------------------

/// How the learning rate falls over a run, as `bullet-train --lr-schedule` takes it.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum LrSchedule {
    #[default]
    Cosine,
    Linear,
    Step,
    Constant,
}

impl LrSchedule {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Cosine => "cosine",
            Self::Linear => "linear",
            Self::Step => "step",
            Self::Constant => "constant",
        }
    }
}

/// A run's schedule, as `bullet-train train` takes it.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize, ToSchema)]
pub struct TrainSchedule {
    pub superbatches: u32,
    pub batches_per_superbatch: u32,
    pub batch_size: u32,
    pub lr: f64,
    pub final_lr: f64,
    #[serde(default)]
    pub lr_schedule: LrSchedule,
    /// The weight of the game result in the target, at the start.
    pub wdl: f64,
    /// ... and at the end (the same as `wdl` when left out).
    #[serde(default)]
    pub final_wdl: Option<f64>,
}

impl Default for TrainSchedule {
    /// A fine-tune: 20 superbatches of 200 batches of 16,384 (65.5 M positions, about 2 minutes on an RTX 3080), the
    /// learning rate a tenth of a from-scratch run's. The smoke run's settings (`trainer/README.md`).
    fn default() -> Self {
        Self {
            superbatches: 20,
            batches_per_superbatch: 200,
            batch_size: 16_384,
            lr: 0.0001,
            final_lr: 0.000001,
            lr_schedule: LrSchedule::Cosine,
            wdl: 0.3,
            final_wdl: None,
        }
    }
}

impl TrainSchedule {
    /// Positions the whole run trains on.
    pub fn positions(&self) -> u64 {
        u64::from(self.superbatches) * u64::from(self.batches_per_superbatch) * u64::from(self.batch_size)
    }

    /// What is wrong with it, for a 400.
    pub fn problem(&self) -> Option<String> {
        if self.superbatches == 0 || self.batches_per_superbatch == 0 || self.batch_size == 0 {
            return Some("A training run needs superbatches, batches and a batch size above zero".into());
        }
        let ok = |x: f64| x.is_finite() && x >= 0.0;
        if !ok(self.lr) || !ok(self.final_lr) || self.lr == 0.0 {
            return Some("The learning rates must be positive numbers".into());
        }
        let wdl = |x: f64| x.is_finite() && (0.0..=1.0).contains(&x);
        if !wdl(self.wdl) || !self.final_wdl.is_none_or(wdl) {
            return Some("wdl must be between 0 and 1".into());
        }
        None
    }
}

/// What a training run trains: the `Train` work's spec, and a `net` idea's `train`.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, ToSchema)]
pub struct TrainSpec {
    /// The data; empty means every dataset in Reckless's internal units.
    #[serde(default)]
    pub datasets: Vec<DatasetId>,
    /// The net to fine-tune; the champion's when left out (unless `from_scratch`).
    #[serde(default)]
    pub parent: Option<NetId>,
    /// Train from random weights instead of fine-tuning.
    #[serde(default)]
    pub from_scratch: bool,
    /// The new net's name; `net-<work id>` when left out.
    #[serde(default)]
    pub name: Option<String>,
    #[serde(default)]
    pub schedule: TrainSchedule,
    /// A checkpoint every this many superbatches (and at the end).
    #[serde(default = "default_save_rate")]
    pub save_rate: u32,
    /// Checkpoints kept, newest first (about 0.72 GB each).
    #[serde(default = "default_keep_checkpoints")]
    pub keep_checkpoints: u32,
}

fn default_save_rate() -> u32 {
    10
}

fn default_keep_checkpoints() -> u32 {
    2
}

impl Default for TrainSpec {
    fn default() -> Self {
        Self {
            datasets: Vec::new(),
            parent: None,
            from_scratch: false,
            name: None,
            schedule: TrainSchedule::default(),
            save_rate: default_save_rate(),
            keep_checkpoints: default_keep_checkpoints(),
        }
    }
}

/// A checkpoint on disk: bullet's optimiser state (weights and Adam moments) plus the quantised net.
pub const CHECKPOINT_BYTES: u64 = 720_000_000;

/// Disk a run needs on top of its data: its checkpoints, the final net and a margin of one more checkpoint (bullet
/// writes the new one before it deletes the oldest).
pub fn run_disk_bytes(spec: &TrainSpec) -> u64 {
    (u64::from(spec.keep_checkpoints.max(1)) + 1) * CHECKPOINT_BYTES + 2 * RECKLESS_NET_BYTES
}

/// One reading of a run, as the loss chart plots it: kept for every `progress` line of the trainer.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize, ToSchema)]
pub struct TrainStep {
    /// From 1, in the order the trainer wrote them.
    pub seq: u32,
    pub superbatch: u32,
    /// Within the superbatch, from 1.
    pub batch: u32,
    /// The mean batch loss since the previous reading.
    pub loss: f64,
    pub lr: f64,
    pub grad_norm: Option<f64>,
    pub positions_per_sec: f64,
    pub elapsed_seconds: f64,
}

// ---------------------------------------------------------------------------------------------------------------------
// Disk
// ---------------------------------------------------------------------------------------------------------------------

/// Free disk that training work always leaves: nothing that writes much starts below it.
pub const KEEP_FREE_BYTES: u64 = 15_000_000_000;

/// The datasets' cap when `DAMELESS_DATA_CAP_GB` does not set one: 10 GB on Windows (a small partition), 200 GB
/// elsewhere.
pub fn default_cap_bytes(windows: bool) -> u64 {
    if windows { 10_000_000_000 } else { 200_000_000_000 }
}

/// Viriformat bytes per Stockfish binpack byte, measured on the same positions (4.32 against 2.01 per position).
pub const VF_PER_BINPACK: f64 = 2.15;

/// What an import of `download_bytes` of binpack will hold on disk: the download while it converts, then the
/// viriformat it becomes.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ImportFootprint {
    /// The most it holds at once: the download and the converted file.
    pub peak: u64,
    /// What the dataset is once converted (the download is deleted).
    pub kept: u64,
}

pub fn import_footprint(download_bytes: u64) -> ImportFootprint {
    let kept = (download_bytes as f64 * VF_PER_BINPACK).ceil() as u64;
    ImportFootprint { peak: download_bytes + kept, kept }
}

/// Whether work that will write `peak` bytes now and keep `kept` bytes of data may start: the datasets stay within
/// `cap` (holding `used` now) and the disk keeps [`KEEP_FREE_BYTES`] free. The error is the sentence people read.
pub fn check_room(peak: u64, kept: u64, used: u64, cap: u64, disk_free: Option<u64>) -> Result<(), String> {
    if used.saturating_add(kept) > cap {
        return Err(format!(
            "The datasets would hold {} of their {} cap ({} now): delete a dataset or raise DAMELESS_DATA_CAP_GB",
            gb(used.saturating_add(kept)),
            gb(cap),
            gb(used)
        ));
    }
    if let Some(free) = disk_free
        && free.saturating_sub(peak) < KEEP_FREE_BYTES
    {
        return Err(format!(
            "Only {} free on the disk: this needs {} and {} must stay free",
            gb(free),
            gb(peak),
            gb(KEEP_FREE_BYTES)
        ));
    }
    Ok(())
}

/// How much more datagen may write: what the cap leaves, and what the disk leaves above [`KEEP_FREE_BYTES`].
pub fn datagen_room(used_by_others: u64, cap: u64, disk_free: Option<u64>, own_bytes: u64) -> u64 {
    let by_cap = cap.saturating_sub(used_by_others);
    match disk_free {
        // The dataset's own bytes are already on the disk: what it may grow to is them plus what is free above the floor.
        Some(free) => by_cap.min(own_bytes + free.saturating_sub(KEEP_FREE_BYTES)),
        None => by_cap,
    }
}

/// `12.3 GB`.
pub fn gb(bytes: u64) -> String {
    format!("{:.1} GB", bytes as f64 / 1e9)
}

// ---------------------------------------------------------------------------------------------------------------------
// Names
// ---------------------------------------------------------------------------------------------------------------------

/// The name of a version that is a champion's code with another net: `v14 + net-3`.
pub fn code_and_net(code: &str, net: &str) -> String {
    format!("{code} + {net}")
}

/// A shipped net's name from its file: `v60-7f587dfb.nnue` → `v60-7f587dfb`.
pub fn net_name_from_file(file: &str) -> String {
    file.strip_suffix(".nnue").unwrap_or(file).to_owned()
}

/// Whether a dataset or net name is safe as a directory or file name: letters, digits, `-`, `_` and `.`, not starting
/// with a dot, at most 64 characters.
pub fn valid_name(name: &str) -> bool {
    !name.is_empty()
        && name.len() <= 64
        && !name.starts_with('.')
        && name.chars().all(|c| c.is_ascii_alphanumeric() || matches!(c, '-' | '_' | '.'))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_default_schedule_is_the_smoke_fine_tune() {
        let s = TrainSchedule::default();
        assert_eq!(s.positions(), 65_536_000);
        assert!(s.problem().is_none());
        assert!(TrainSchedule { superbatches: 0, ..s }.problem().is_some());
        assert!(TrainSchedule { wdl: 1.5, ..s }.problem().is_some());
        assert!(TrainSchedule { final_wdl: Some(-0.1), ..s }.problem().is_some());
        let spec: TrainSpec = serde_json::from_value(serde_json::json!({})).unwrap();
        assert_eq!(spec, TrainSpec::default());
        assert_eq!((spec.save_rate, spec.keep_checkpoints), (10, 2));
        assert_eq!(serde_json::to_value(LrSchedule::Cosine).unwrap(), "cosine");
        assert_eq!(run_disk_bytes(&spec), 3 * CHECKPOINT_BYTES + 2 * RECKLESS_NET_BYTES);
    }

    #[test]
    fn room_keeps_the_cap_and_the_disk_floor() {
        let cap = default_cap_bytes(true);
        assert_eq!(cap, 10_000_000_000);
        assert_eq!(default_cap_bytes(false), 200_000_000_000);
        assert!(check_room(1_000, 1_000, 0, cap, Some(100_000_000_000)).is_ok());
        let over = check_room(1_000, 3_000_000_000, 8_000_000_000, cap, None).unwrap_err();
        assert!(over.contains("11.0 GB of their 10.0 GB cap"), "{over}");
        let full = check_room(6_000_000_000, 1, 0, cap, Some(20_000_000_000)).unwrap_err();
        assert!(full.contains("Only 20.0 GB free"), "{full}");
        let f = import_footprint(1_000_000_000);
        assert_eq!((f.kept, f.peak), (2_150_000_000, 3_150_000_000));
    }

    #[test]
    fn datagen_grows_to_what_the_cap_and_disk_leave() {
        let cap = 10_000_000_000;
        assert_eq!(datagen_room(2_000_000_000, cap, None, 0), 8_000_000_000);
        // 16 GB free: 1 GB above the floor, plus the 3 GB it already has.
        assert_eq!(datagen_room(0, cap, Some(16_000_000_000), 3_000_000_000), 4_000_000_000);
        assert_eq!(datagen_room(12_000_000_000, cap, None, 0), 0);
    }

    #[test]
    fn names() {
        assert_eq!(code_and_net("v14", "net-3"), "v14 + net-3");
        assert_eq!(net_name_from_file("v60-7f587dfb.nnue"), "v60-7f587dfb");
        assert!(valid_name("dfrc-5k_1.v2"));
        assert!(!valid_name("../x") && !valid_name(".hidden") && !valid_name("") && !valid_name("a b"));
        let source = DatasetSource::Import { url: "http://x/a.binpack".into() };
        assert_eq!(serde_json::to_value(&source).unwrap()["type"], "import");
    }
}
