use std::{sync::atomic::Ordering, time::Duration};

use dameless_core::{
    ErrorCode,
    harness::Machine,
    ideas::{Stage, TrialStatus},
};
use dameless_engine::git;
use dameless_hub::Hub;

use super::*;
use crate::{
    scheduler::Scheduler,
    testing::{Fake, FakeProbes, TestRepo, hub, repo},
    versions::{VersionRef, Versions},
};

struct Lab {
    repo: TestRepo,
    hub: Arc<Hub>,
    _dir: tempfile::TempDir,
    harness: Harness,
    fake: Arc<Fake>,
    probes: Arc<FakeProbes>,
}

async fn lab() -> Lab {
    let repo = repo();
    // Reckless keeps its entry point in src/main.rs; the fake bench does not read it.
    repo.commit("main", "main", &[("src/main.rs", "fn main() {}\n")], None);
    let (hub, dir) = hub(&repo, 2);
    let scheduler = Scheduler::new(hub.clone());
    let fake = Fake::new();
    let versions = Versions::with_toolchain(hub.clone(), scheduler.clone(), fake.clone());
    scheduler.register(versions.executor());
    let ideas = Ideas::new(versions);
    ideas.register();
    scheduler.start().await.unwrap();
    let probes = FakeProbes::new();
    let harness = Harness::with_probes(ideas, probes.clone());
    Lab { repo, hub, _dir: dir, harness, fake, probes }
}

async fn ended(lab: &Lab, id: IdeaId) -> Idea {
    for _ in 0..2000 {
        let idea = lab.harness.ideas().get(id).await.unwrap();
        if idea.status.is_final() {
            return idea;
        }
        tokio::time::sleep(Duration::from_millis(10)).await;
    }
    panic!("harness check {id} did not end");
}

/// Even pairs with some spread, at Elo 0.
const EVEN: [u32; 5] = [1, 1, 2, 1, 1];
/// The change scores a little better: an identical engine should never look like this.
const BETTER: [u32; 5] = [1, 1, 2, 1, 2];

#[tokio::test]
async fn the_identical_branch_changes_only_a_comment_and_keeps_the_bench() {
    let lab = lab().await;
    let main = lab.repo.git(&["rev-parse", "main"]);
    let made_from = lab.harness.make_identical().await.unwrap();
    assert_eq!(made_from, main);
    assert_eq!(lab.repo.git(&["rev-parse", &format!("{}^", harness::BRANCH)]), main);
    assert_eq!(lab.repo.git(&["diff", "--name-only", "main", harness::BRANCH]), "src/main.rs");
    let diff = lab.repo.git(&["diff", "-U0", "main", harness::BRANCH, "--", "src/main.rs"]);
    let added: Vec<&str> = diff.lines().filter(|l| l.starts_with('+') && !l.starts_with("+++")).collect();
    assert!(!added.is_empty() && added.iter().all(|l| *l == "+" || l.starts_with("+//")), "{diff}");
    let removed = diff.lines().filter(|l| l.starts_with('-') && !l.starts_with("---")).count();
    assert_eq!(removed, 0, "{diff}");

    // Its Bench: line is main's, and so is what it benches.
    let message = lab.repo.git(&["log", "-1", "--format=%B", harness::BRANCH]);
    let main_message = lab.repo.git(&["log", "-1", "--format=%B", "main"]);
    assert!(git::bench_line(&message).is_some());
    assert_eq!(git::bench_line(&message), git::bench_line(&main_message));
    let versions = lab.harness.ideas().versions();
    let (identical, _) = versions.binary(&VersionRef::Name(harness::BRANCH.into()), &[]).await.unwrap();
    let (champion, _) = versions.binary(&VersionRef::Champion, &[]).await.unwrap();
    assert_eq!(identical.bench, champion.bench);
    assert_eq!(identical.bench, git::bench_line(&message));

    // The user's checkout is untouched.
    assert_eq!(lab.repo.git(&["symbolic-ref", "--short", "HEAD"]), "main");
    assert_eq!(lab.repo.git(&["status", "--porcelain"]), "");

    // Made again from main's current tip.
    let moved = lab.repo.commit("main", "main", &[("src/search.rs", "fn search() { deeper(); }\n")], None);
    lab.harness.make_identical().await.unwrap();
    assert_eq!(lab.repo.git(&["rev-parse", &format!("{}^", harness::BRANCH)]), moved);
    assert_eq!(lab.repo.git(&["rev-list", "--count", &format!("main..{}", harness::BRANCH)]), "1");
}

#[tokio::test]
async fn an_identical_engine_rejected_near_zero_passes_the_check() {
    let lab = lab().await;
    lab.fake.rounds(EVEN);
    // Each round reports a thousand times its pairs, so the SPRT between equals decides in a few rounds.
    lab.fake.pair_scale.store(1000, Ordering::SeqCst);
    let before = lab.harness.status().await.unwrap();
    assert!(!before.checked && !before.trusted && before.summary.contains("unchecked"), "{before:?}");

    let idea = lab.harness.start_check(HarnessCheckRequest { pairs: Some(harness::SYMMETRY_PAIRS) }).await.unwrap();
    assert_eq!(idea.branch, harness::BRANCH);
    assert_eq!(idea.tags, [harness::TAG]);
    assert_eq!(idea.trials.iter().map(|t| t.stage).collect::<Vec<_>>(), [Stage::Custom, Stage::Custom]);
    assert_eq!(idea.trials[0].bounds, Some(harness::BOUNDS));
    assert_eq!(idea.trials[0].preset, Some(harness::PRESET));
    if !lab.harness.ideas().get(idea.id).await.unwrap().status.is_final() {
        let e = lab.harness.start_check(HarnessCheckRequest::default()).await.unwrap_err();
        assert_eq!(e.code(), ErrorCode::Conflict);
    }

    let idea = ended(&lab, idea.id).await;
    assert_eq!(idea.status, IdeaStatus::Kept, "{:?}", idea.verdict);
    let verdict = idea.verdict.clone().unwrap();
    assert!(verdict.starts_with("Harness checked: An identical engine was rejected"), "{verdict}");
    assert!(verdict.contains("Symmetric over"), "{verdict}");
    // The SPRT trial rejected (its own verdict), and the symmetry run played its pairs.
    assert_eq!(idea.trials[0].status, TrialStatus::Failed);
    assert!(idea.trials[0].detail.as_deref().unwrap().starts_with("rejected [0, 3]"));
    assert_eq!(idea.trials[1].status, TrialStatus::Passed);

    let check: HarnessCheck = lab.hub.store.meta(META_KEY).await.unwrap().unwrap();
    assert!(check.passed && check.idea == idea.id);
    assert_eq!(check.machine, lab.probes.machine.lock().unwrap().clone());
    assert_eq!(check.engine_commit, lab.repo.git(&["rev-parse", "main"]));
    assert_eq!((check.fastchess.as_deref(), check.book.as_deref()), (Some("fastchess"), Some("book.epd")));
    let elo = check.elo.unwrap();
    assert!(elo.low <= 0.0 && elo.high >= 0.0);
    assert!(check.symmetry.as_ref().is_some_and(|s| s.passed));

    let status = lab.harness.status().await.unwrap();
    assert!(status.checked && status.current && status.trusted && status.running.is_none(), "{status:?}");
    assert_eq!((status.idea, status.passed), (Some(idea.id), Some(true)));

    // A harness check is no change to promote.
    assert_eq!(lab.harness.ideas().promote(idea.id).await.unwrap_err().code(), ErrorCode::Conflict);
}

#[tokio::test]
async fn a_second_check_waits_for_the_first() {
    let lab = lab().await;
    lab.fake.rounds(EVEN);
    *lab.fake.round_delay.lock().unwrap() = Duration::from_secs(30);
    let idea = lab.harness.start_check(HarnessCheckRequest::default()).await.unwrap();
    assert_eq!(lab.harness.status().await.unwrap().running, Some(idea.id));
    let e = lab.harness.start_check(HarnessCheckRequest::default()).await.unwrap_err();
    assert_eq!(e.code(), ErrorCode::Conflict);
    lab.harness.ideas().stop(idea.id).await.unwrap();
}

#[tokio::test]
async fn an_identical_engine_accepted_fails_the_check() {
    let lab = lab().await;
    lab.fake.rounds(BETTER);
    lab.fake.pair_scale.store(100, Ordering::SeqCst);
    let idea = lab.harness.start_check(HarnessCheckRequest { pairs: None }).await.unwrap();
    assert_eq!(idea.trials.len(), 1);
    let idea = ended(&lab, idea.id).await;
    assert_eq!(idea.status, IdeaStatus::Dropped);
    let verdict = idea.verdict.unwrap();
    assert!(verdict.starts_with("Harness check failed: An identical engine was accepted"), "{verdict}");
    assert!(verdict.contains("do not trust results until this is investigated"));
    let status = lab.harness.status().await.unwrap();
    assert!(status.checked && status.current && !status.trusted && status.passed == Some(false));
    assert!(status.summary.starts_with("The harness check failed"), "{}", status.summary);
    let check: HarnessCheck = lab.hub.store.meta(META_KEY).await.unwrap().unwrap();
    assert!(!check.passed && check.symmetry.is_none());
}

#[tokio::test]
async fn the_status_goes_stale_on_another_machine_or_other_tools() {
    let lab = lab().await;
    lab.fake.rounds(EVEN);
    lab.fake.pair_scale.store(1000, Ordering::SeqCst);
    let idea = lab.harness.start_check(HarnessCheckRequest::default()).await.unwrap();
    ended(&lab, idea.id).await;
    assert!(lab.harness.status().await.unwrap().trusted);

    let here = lab.probes.machine.lock().unwrap().clone();
    lab.probes.machine.lock().unwrap().cpu = "Another CPU".into();
    let moved = lab.harness.status().await.unwrap();
    assert!(moved.checked && !moved.current && !moved.trusted, "{moved:?}");
    assert!(moved.summary.contains("another machine"));
    *lab.probes.machine.lock().unwrap() = Machine { cores: 16, ..here.clone() };
    assert!(!lab.harness.status().await.unwrap().current);
    *lab.probes.machine.lock().unwrap() = here;
    assert!(lab.harness.status().await.unwrap().current);

    *lab.fake.fastchess_name.lock().unwrap() = Some("fastchess-1.9.0".into());
    let tools = lab.harness.status().await.unwrap();
    assert!(!tools.current && !tools.trusted && tools.summary.contains("another fastchess"), "{tools:?}");
}

#[tokio::test]
async fn a_check_needs_the_tools() {
    let lab = lab().await;
    lab.fake.missing_tools.store(true, Ordering::SeqCst);
    let e = lab.harness.start_check(HarnessCheckRequest::default()).await.unwrap_err();
    assert_eq!(e.code(), ErrorCode::Conflict);
    assert!(e.message().contains("dameless setup"));
    let zero = lab.harness.start_check(HarnessCheckRequest { pairs: Some(0) }).await.unwrap_err();
    assert_eq!(zero.code(), ErrorCode::BadRequest);
}

#[test]
fn tools_are_named_by_their_pinned_file() {
    assert_eq!(tool_name(Path::new("tools/fastchess-1.8.2.exe")), "fastchess-1.8.2");
    assert_eq!(tool_name(Path::new("tools/UHO_Lichess_4852_v1.epd")), "UHO_Lichess_4852_v1.epd");
}
