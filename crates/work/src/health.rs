//! Health: what Settings shows about the machine, one item per thing that has to be right for the app to work: the
//! analysis engine, the Maia engines, free disk and the database.
//!
//! Probing the disk goes through [`Probes`], so tests inject what they need and never depend on the machine they run
//! on.

use std::{
    path::{Path, PathBuf},
    sync::Arc,
};

use dameless_core::Error;
use serde::{Deserialize, Serialize};
use utoipa::ToSchema;

use crate::engines::Engines;

/// Free disk under this is a warning: games and the position cache need room.
pub const DISK_WARN_BYTES: u64 = 5 * GB;
/// Free disk under this fails: the database may not be able to grow.
pub const DISK_FAIL_BYTES: u64 = GB;
const GB: u64 = 1_000_000_000;

/// Space on the drive holding a path.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
pub struct DiskSpace {
    pub free_bytes: u64,
    pub total_bytes: u64,
}

/// What the machine has. [`SystemProbes`] asks the real one.
pub trait Probes: Send + Sync {
    /// Space on the drive that holds `path`.
    fn disk(&self, path: &Path) -> Option<DiskSpace>;
}

/// The real machine, through sysinfo.
#[derive(Default)]
pub struct SystemProbes;

impl Probes for SystemProbes {
    fn disk(&self, path: &Path) -> Option<DiskSpace> {
        let path = path.ancestors().find(|p| p.exists())?.to_path_buf();
        let path = std::fs::canonicalize(&path).unwrap_or(path);
        let text = path.to_string_lossy();
        // Windows' canonical form starts with \\?\, which mount points do not.
        let path = PathBuf::from(text.strip_prefix(r"\\?\").unwrap_or(&text));
        let disks = sysinfo::Disks::new_with_refreshed_list();
        disks
            .list()
            .iter()
            .filter(|d| path.starts_with(d.mount_point()))
            .max_by_key(|d| d.mount_point().as_os_str().len())
            .map(|d| DiskSpace { free_bytes: d.available_space(), total_bytes: d.total_space() })
    }
}

/// How an item stands.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum HealthLevel {
    Ok,
    /// For your information: not needed for what the app does now (the Maia engines).
    Info,
    /// Works, but needs attention soon (low disk).
    Warn,
    /// Broken: something will fail until it is fixed.
    Fail,
}

/// What fixes an item: a label for the button, and the CLI command that does the same.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
pub struct HealthAction {
    pub label: String,
    pub command: String,
}

/// One thing Settings shows.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
pub struct HealthItem {
    /// `analysis`, `maia`, `disk`, `database`.
    pub key: String,
    /// Not failing (a warning or a note is still ok).
    pub ok: bool,
    pub level: HealthLevel,
    pub title: String,
    /// One sentence for people.
    pub detail: String,
    pub action: Option<HealthAction>,
}

/// `GET /api/diagnostics`: every item, and whether none fails.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
pub struct Health {
    pub ok: bool,
    pub items: Vec<HealthItem>,
}

fn item(key: &str, level: HealthLevel, title: &str, detail: String, action: Option<(&str, &str)>) -> HealthItem {
    HealthItem {
        key: key.to_owned(),
        ok: level != HealthLevel::Fail,
        level,
        title: title.to_owned(),
        detail,
        action: action.map(|(label, command)| HealthAction { label: label.to_owned(), command: command.to_owned() }),
    }
}

/// Bytes for people: "12.3 GB".
pub fn bytes_text(n: u64) -> String {
    const UNITS: [&str; 5] = ["B", "kB", "MB", "GB", "TB"];
    let mut v = n as f64;
    let mut unit = 0;
    while v >= 1000.0 && unit < UNITS.len() - 1 {
        v /= 1000.0;
        unit += 1;
    }
    if unit == 0 { format!("{n} B") } else { format!("{v:.1} {}", UNITS[unit]) }
}

/// Health of the machine and the app. Cheap to clone.
#[derive(Clone)]
pub struct Diagnostics {
    engines: Engines,
    probes: Arc<dyn Probes>,
}

impl Diagnostics {
    pub fn new(engines: Engines, probes: Arc<dyn Probes>) -> Self {
        Self { engines, probes }
    }

    /// Every item, in the order Settings shows them.
    pub async fn health(&self) -> Result<Health, Error> {
        let hub = self.engines.hub().clone();
        let mut items = Vec::new();
        let setup = Some(("Install", "dameless setup"));
        let overview = self.engines.overview().await?;
        let analysis = overview.engines.iter().find(|e| e.id == overview.analysis);
        items.push(match analysis {
            Some(e) if e.installed => item(
                "analysis",
                HealthLevel::Ok,
                "Analysis engine",
                format!("{}: reviews, evaluation and the levels run on it", e.label()),
                None,
            ),
            Some(e) => item(
                "analysis",
                HealthLevel::Fail,
                "Analysis engine",
                format!("{} is not installed: reviews, evaluation and the levels cannot run", e.name),
                setup,
            ),
            None => item(
                "analysis",
                HealthLevel::Fail,
                "Analysis engine",
                format!("No engine {}: choose another in Settings", overview.analysis),
                None,
            ),
        });
        let maias: Vec<_> = overview.engines.iter().filter(|e| e.id.starts_with("maia-")).collect();
        let ready = maias.iter().filter(|e| e.installed).count();
        items.push(if ready == maias.len() && ready > 0 {
            item("maia", HealthLevel::Ok, "Maia", format!("{ready} Maia engines installed"), None)
        } else {
            item(
                "maia",
                HealthLevel::Info,
                "Maia",
                format!("{ready} of {} Maia engines installed: they play like people at their rating", maias.len()),
                setup,
            )
        });
        items.push(match self.probes.disk(&hub.config.data_dir) {
            Some(d) => {
                let level = if d.free_bytes < DISK_FAIL_BYTES {
                    HealthLevel::Fail
                } else if d.free_bytes < DISK_WARN_BYTES {
                    HealthLevel::Warn
                } else {
                    HealthLevel::Ok
                };
                let detail = format!(
                    "{} free of {} on the data directory's drive{}",
                    bytes_text(d.free_bytes),
                    bytes_text(d.total_bytes),
                    if level == HealthLevel::Ok { "" } else { ": the database needs room to grow" }
                );
                item("disk", level, "Disk", detail, None)
            }
            None => item("disk", HealthLevel::Warn, "Disk", "Free space could not be read".into(), None),
        });
        let db = hub.store.path().to_path_buf();
        let db_bytes = [db.clone(), PathBuf::from(format!("{}-wal", db.display()))]
            .iter()
            .filter_map(|p| std::fs::metadata(p).ok())
            .map(|m| m.len())
            .sum::<u64>();
        items.push(item(
            "database",
            HealthLevel::Ok,
            "Database",
            format!("{} at {}", bytes_text(db_bytes), db.display()),
            None,
        ));
        Ok(Health { ok: items.iter().all(|i| i.ok), items })
    }
}

#[cfg(test)]
mod tests;
