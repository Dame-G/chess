use std::sync::atomic::Ordering;

use super::*;
use crate::{
    harness::Harness,
    ideas::Ideas,
    scheduler::Scheduler,
    testing::{Fake, FakeProbes, bare_hub, hub, repo},
    versions::Versions,
};

fn diagnostics(hub: Arc<dameless_hub::Hub>, fake: Arc<Fake>, probes: Arc<FakeProbes>) -> Diagnostics {
    let scheduler = Scheduler::new(hub.clone());
    let versions = Versions::with_toolchain(hub, scheduler, fake);
    Diagnostics::new(Harness::with_probes(Ideas::new(versions), probes))
}

fn find<'a>(h: &'a Health, key: &str) -> &'a HealthItem {
    h.items.iter().find(|i| i.key == key).unwrap_or_else(|| panic!("no {key} item"))
}

#[tokio::test]
async fn a_healthy_machine_lists_every_item() {
    let repo = repo();
    let (hub, _dir) = hub(&repo, 2);
    let (fake, probes) = (Fake::new(), FakeProbes::new());
    let builds = hub.config.builds_dir();
    for name in ["aaaa1111-plain", "bbbb2222-spsa"] {
        std::fs::create_dir_all(builds.join(name)).unwrap();
        std::fs::write(builds.join(name).join(dameless_engine::exe("reckless")), vec![0u8; 1500]).unwrap();
    }
    std::fs::create_dir_all(builds.join("target")).unwrap();
    let d = diagnostics(hub, fake, probes);
    let h = d.health().await.unwrap();
    let keys: Vec<&str> = h.items.iter().map(|i| i.key.as_str()).collect();
    assert_eq!(keys, ["fastchess", "book", "engine", "clang", "cuda", "gpu", "disk", "database", "builds", "harness"]);
    assert!(h.ok, "{h:?}");
    assert!(find(&h, "engine").detail.contains("main at "));
    assert!(find(&h, "clang").detail.contains("Syzygy"));
    assert_eq!(find(&h, "cuda").level, HealthLevel::Ok);
    assert_eq!(find(&h, "gpu").detail, "NVIDIA GeForce RTX 3080, 10.7 GB of memory");
    assert_eq!(find(&h, "disk").detail, "500.0 GB free of 1.0 TB on the data directory's drive");
    assert_eq!(find(&h, "builds").detail, "2 builds on disk, 3.0 kB");
    assert!(find(&h, "database").detail.ends_with("dameless.db") || find(&h, "database").detail.contains(" at "));
    // Never checked here: a warning, with the way to check it.
    let harness = find(&h, "harness");
    assert_eq!(harness.level, HealthLevel::Warn);
    assert!(harness.ok && harness.detail.contains("unchecked"));
    assert_eq!(harness.action.as_ref().unwrap().command, "dameless harness check");
}

#[tokio::test]
async fn missing_and_low_things_warn_or_fail() {
    let repo = repo();
    let (hub, _dir) = hub(&repo, 2);
    let (fake, probes) = (Fake::new(), FakeProbes::new());
    fake.missing_tools.store(true, Ordering::SeqCst);
    *probes.clang.lock().unwrap() = None;
    *probes.cuda.lock().unwrap() = Cuda::default();
    *probes.gpu.lock().unwrap() = None;
    *probes.disk.lock().unwrap() = Some(DiskSpace { free_bytes: 12 * GB, total_bytes: 500 * GB });
    let d = diagnostics(hub, fake, probes.clone());
    let h = d.health().await.unwrap();
    assert!(!h.ok);
    for key in ["fastchess", "book"] {
        let i = find(&h, key);
        assert_eq!((i.level, i.ok), (HealthLevel::Fail, false));
        assert_eq!(i.action.as_ref().unwrap().command, "dameless setup");
    }
    assert_eq!(find(&h, "clang").level, HealthLevel::Warn);
    assert!(find(&h, "clang").detail.contains("without Syzygy"));
    assert_eq!((find(&h, "cuda").level, find(&h, "gpu").level), (HealthLevel::Info, HealthLevel::Info));
    assert!(find(&h, "cuda").ok && find(&h, "cuda").detail.contains("only for training"));
    let disk = find(&h, "disk");
    assert_eq!((disk.level, disk.ok), (HealthLevel::Warn, true));
    assert_eq!(disk.action.as_ref().unwrap().command, "dameless builds prune");

    // The slow probes are kept for a minute: a change shows only once they are asked again.
    *probes.disk.lock().unwrap() = Some(DiskSpace { free_bytes: 3 * GB, total_bytes: 500 * GB });
    *probes.clang.lock().unwrap() = Some("clang version 19".into());
    let h = d.health().await.unwrap();
    assert_eq!(probes.slow_calls.load(Ordering::SeqCst), 1);
    assert_eq!(find(&h, "clang").level, HealthLevel::Warn);
    let disk = find(&h, "disk");
    assert_eq!((disk.level, disk.ok), (HealthLevel::Fail, false));
}

#[tokio::test]
async fn no_engine_checkout_fails() {
    let (hub, _dir) = bare_hub(2);
    let d = diagnostics(hub, Fake::new(), FakeProbes::new());
    let engine = find(&d.health().await.unwrap(), "engine").clone();
    assert_eq!(engine.level, HealthLevel::Fail);
    assert!(engine.detail.contains("RECKLESS_DIR"), "{}", engine.detail);
}

#[test]
fn nvidia_smi_and_sizes_read_as_people_read_them() {
    assert_eq!(
        parse_gpu("NVIDIA GeForce RTX 3080, 10240\n"),
        Some(Gpu { name: "NVIDIA GeForce RTX 3080".into(), memory_mb: Some(10240) })
    );
    assert_eq!(parse_gpu("\n"), None);
    assert_eq!(bytes_text(999), "999 B");
    assert_eq!(bytes_text(20 * GB), "20.0 GB");
}
