use super::*;
use crate::testing::bare_hub;

struct Disk(Option<DiskSpace>);

impl Probes for Disk {
    fn disk(&self, _: &Path) -> Option<DiskSpace> {
        self.0
    }
}

fn find<'a>(h: &'a Health, key: &str) -> &'a HealthItem {
    h.items.iter().find(|i| i.key == key).unwrap_or_else(|| panic!("no {key} item"))
}

#[tokio::test]
async fn a_fresh_app_asks_for_setup() {
    let (hub, _dir) = bare_hub(2);
    let free = DiskSpace { free_bytes: 100 * GB, total_bytes: 500 * GB };
    let d = Diagnostics::new(Engines::new(hub), Arc::new(Disk(Some(free))));
    let h = d.health().await.unwrap();
    assert_eq!(
        h.items.iter().map(|i| i.key.as_str()).collect::<Vec<_>>(),
        ["analysis", "reckless", "disk", "database"]
    );
    let analysis = find(&h, "analysis");
    assert_eq!(analysis.level, HealthLevel::Fail);
    assert_eq!(analysis.action.as_ref().unwrap().command, "chess setup");
    assert_eq!(find(&h, "reckless").level, HealthLevel::Info);
    assert_eq!(find(&h, "reckless").action.as_ref().unwrap().command, "chess setup --only reckless");
    assert_eq!(find(&h, "disk").level, HealthLevel::Ok);
    assert!(!h.ok);
}

#[tokio::test]
async fn low_disk_warns_then_fails() {
    let (hub, _dir) = bare_hub(2);
    let low = DiskSpace { free_bytes: 2 * GB, total_bytes: 500 * GB };
    let h = Diagnostics::new(Engines::new(hub.clone()), Arc::new(Disk(Some(low)))).health().await.unwrap();
    assert_eq!(find(&h, "disk").level, HealthLevel::Warn);
    let full = DiskSpace { free_bytes: GB / 2, total_bytes: 500 * GB };
    let h = Diagnostics::new(Engines::new(hub.clone()), Arc::new(Disk(Some(full)))).health().await.unwrap();
    assert_eq!(find(&h, "disk").level, HealthLevel::Fail);
    let h = Diagnostics::new(Engines::new(hub), Arc::new(Disk(None))).health().await.unwrap();
    assert_eq!(find(&h, "disk").level, HealthLevel::Warn);
    assert_eq!(bytes_text(1_500_000), "1.5 MB");
}
