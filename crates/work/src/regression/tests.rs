use std::time::Duration;

use dameless_core::ideas::{Preset, TrialStatus};
use dameless_hub::Hub;

use super::*;
use crate::{
    scheduler::Scheduler,
    testing::{Fake, TestRepo, hub, repo},
    versions::Versions,
};

struct Lab {
    repo: TestRepo,
    hub: Arc<Hub>,
    _dir: tempfile::TempDir,
    regression: Regression,
    fake: Arc<Fake>,
}

async fn lab() -> Lab {
    let repo = repo();
    let (hub, dir) = hub(&repo, 2);
    let scheduler = Scheduler::new(hub.clone());
    let fake = Fake::new();
    let versions = Versions::with_toolchain(hub.clone(), scheduler.clone(), fake.clone());
    scheduler.register(versions.executor());
    let ideas = Ideas::new(versions);
    ideas.register();
    scheduler.start().await.unwrap();
    let regression = Regression::with_pairs(ideas, 40);
    regression.register();
    Lab { repo, hub, _dir: dir, regression, fake }
}

impl Lab {
    fn ideas(&self) -> &Ideas {
        &self.regression.ideas
    }

    /// Moves `main` on `n` times, each a new champion.
    async fn promote(&self, n: usize) -> Version {
        let versions = self.ideas().versions();
        let mut champion = versions.champion().await.unwrap();
        for _ in 0..n {
            let count = self.repo.git(&["rev-list", "--count", "main"]);
            self.repo.commit("main", "main", &[("src/search.rs", &format!("fn search() {{ /* {count} */ }}\n"))], None);
            champion = versions.champion().await.unwrap();
        }
        champion
    }

    async fn ended(&self, id: IdeaId) -> Idea {
        for _ in 0..2000 {
            let idea = self.ideas().get(id).await.unwrap();
            if idea.status.is_final() {
                return idea;
            }
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
        panic!("regression check {id} did not end");
    }
}

/// Even pairs with some spread, at Elo 0.
const EVEN: [u32; 5] = [1, 1, 2, 1, 1];
/// The newer champion loses every pair.
const LOSSES: [u32; 5] = [1, 0, 0, 0, 0];

#[tokio::test]
async fn a_check_is_due_after_every_fifth_promotion_and_only_once() {
    let lab = lab().await;
    lab.fake.rounds(EVEN);
    let v5 = lab.promote(4).await;
    assert_eq!(v5.name, "v5");
    assert_eq!(lab.regression.queue_if_due().await.unwrap(), None);
    let v6 = lab.promote(1).await;
    let idea = lab.regression.queue_if_due().await.unwrap().expect("v6 is due a check against v1");
    assert_eq!(idea.title, "Regression check: v6 against v1");
    assert_eq!(idea.version, Some(v6.id));
    let v1 = lab.hub.store.versions(Some("champion")).await.unwrap().into_iter().find(|v| v.name == "v1").unwrap();
    assert_eq!(idea.base, v1.id);
    assert_eq!(idea.tags, [regression::TAG]);
    assert_eq!(idea.trials.len(), 1);
    assert_eq!((idea.trials[0].stage, idea.trials[0].preset), (Stage::Regression, Some(Preset::Stc)));
    assert_eq!(idea.trials[0].bounds, None);
    // Asking again queues nothing: v6 has its check.
    assert_eq!(lab.regression.queue_if_due().await.unwrap(), None);

    let idea = lab.ended(idea.id).await;
    assert_eq!(idea.status, IdeaStatus::Kept, "{:?}", idea.verdict);
    assert!(idea.verdict.as_deref().unwrap().starts_with("v6 over v1: "), "{:?}", idea.verdict);
    let checks = lab.regression.checks().await.unwrap();
    assert_eq!(checks.len(), 1);
    assert!(checks[0].passed && checks[0].pairs >= 40 && checks[0].elo.is_some());
    assert_eq!((checks[0].champion, checks[0].against), (v6.id, v1.id));
    // The champion's own Elo (over its parent) is left alone.
    assert_eq!(lab.hub.store.version(v6.id).await.unwrap().unwrap().elo, None);
    // Not a change to promote.
    assert_eq!(lab.ideas().promote(idea.id).await.unwrap_err().status(), 409);
}

#[tokio::test]
async fn a_measurably_worse_champion_fails_loudly_and_nothing_is_reverted() {
    let lab = lab().await;
    lab.fake.rounds(LOSSES);
    let v6 = lab.promote(5).await;
    let idea = lab.regression.queue_if_due().await.unwrap().unwrap();
    let idea = lab.ended(idea.id).await;
    assert_eq!(idea.status, IdeaStatus::Dropped);
    let verdict = idea.verdict.unwrap();
    assert!(verdict.starts_with("Regression: v6 is measurably worse than v1"), "{verdict}");
    let trial = lab.ideas().trial(idea.id, idea.trials[0].id).await.unwrap();
    assert_eq!(trial.status, TrialStatus::Failed);
    let checks = lab.regression.checks().await.unwrap();
    assert!(!checks[0].passed && checks[0].elo.unwrap().high < 0.0);
    // The champion is still v6.
    assert_eq!(lab.ideas().versions().champion().await.unwrap().id, v6.id);
}
