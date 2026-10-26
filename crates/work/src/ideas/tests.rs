use std::{sync::atomic::Ordering, time::Duration};

use dameless_core::{
    events::EventKind,
    game::Source,
    ideas::{IdeaKind, IdeaStatus, Preset, Stage, TrialStatus},
    version::Line,
};
use dameless_hub::{Next as BusNext, TopicFilter};
use dameless_store::GameFilter;

use super::*;
use crate::testing::{Fake, PARAMETERS, TestRepo, eventually, hub, repo};

struct Lab {
    repo: TestRepo,
    hub: Arc<Hub>,
    _dir: tempfile::TempDir,
    scheduler: Scheduler,
    ideas: Ideas,
    fake: Arc<Fake>,
}

async fn lab(cores: u32) -> Lab {
    let repo = repo();
    let (hub, dir) = hub(&repo, cores);
    let scheduler = Scheduler::new(hub.clone());
    let fake = Fake::new();
    let versions = Versions::with_toolchain(hub.clone(), scheduler.clone(), fake.clone());
    scheduler.register(versions.executor());
    let ideas = Ideas::new(versions);
    ideas.register();
    scheduler.start().await.unwrap();
    Lab { repo, hub, _dir: dir, scheduler, ideas, fake }
}

fn request(title: &str, kind: IdeaKind, branch: &str) -> NewIdeaRequest {
    NewIdeaRequest {
        title: title.into(),
        hypothesis: "It helps".into(),
        kind,
        branch: branch.into(),
        base: None,
        author: Author::Claude,
        tags: vec![],
        notes: None,
        long: false,
        tune: None,
        custom: None,
        fix_bench: false,
    }
}

async fn ended(lab: &Lab, id: IdeaId) -> Idea {
    let mut idea = lab.ideas.get(id).await.unwrap();
    for _ in 0..1000 {
        if idea.status.is_final() {
            return idea;
        }
        tokio::time::sleep(Duration::from_millis(10)).await;
        idea = lab.ideas.get(id).await.unwrap();
    }
    panic!("idea {id} did not end: {idea:?}");
}

const WINS: [u32; 5] = [0, 0, 0, 0, 1];
const LOSSES: [u32; 5] = [1, 0, 0, 0, 0];
const DRAWS: [u32; 5] = [0, 0, 1, 0, 0];
/// Even games with some spread: the SPRT moves slowly.
const EVEN: [u32; 5] = [1, 1, 2, 1, 1];

#[tokio::test]
async fn a_gain_runs_its_funnel_and_is_kept() {
    let lab = lab(2).await;
    lab.repo.commit("deeper-lmr", "main", &[("src/search.rs", "fn search() { lmr(); }\n")], None);
    lab.fake.rounds(WINS);
    let mut sub = lab.hub.bus.subscribe(0, TopicFilter::of(["ideas", "idea:*", "games", "work"]));
    let idea = lab.ideas.create(request("Deeper LMR", IdeaKind::Gain, "deeper-lmr")).await.unwrap();
    assert_eq!(
        idea.trials.iter().map(|t| t.stage).collect::<Vec<_>>(),
        [Stage::Fingerprint, Stage::Screen, Stage::Short]
    );
    assert!(idea.version.is_some());
    let idea = ended(&lab, idea.id).await;
    assert_eq!(idea.status, IdeaStatus::Kept, "{:?}", idea.verdict);
    let verdict = idea.verdict.clone().unwrap();
    assert!(verdict.starts_with("Kept: +") && verdict.ends_with("Elo at short games"), "{verdict}");
    assert!(idea.trials.iter().all(|t| t.status == TrialStatus::Passed));
    assert!(idea.trials[0].detail.as_deref().unwrap().starts_with("bench "));
    assert!(idea.ended_at.is_some());

    // Rounds of four pairs per core (two cores, one thread a game).
    let played = lab.fake.played();
    assert!(played.iter().all(|p| p.pairs == 8 && p.concurrency == 2), "{played:?}");
    // Every game of every round is stored, numbered within its trial, onto the board.
    let short = lab.ideas.trial(idea.id, idea.trials[2].id).await.unwrap();
    assert_eq!(short.played, Some(Preset::Stc.settings().time), "the fake benches at the reference speed");
    assert!(short.scaling.is_some() && short.llr.is_some() && short.elo.is_some());
    let games =
        lab.hub.store.games(GameFilter { trial: Some(short.id), limit: 5000, ..Default::default() }).await.unwrap();
    assert_eq!(games.len() as u64, short.penta.pairs() * 2);
    let mut indexes: Vec<u32> = games
        .iter()
        .map(|g| match g.source {
            Source::Trial { index, .. } => index,
            _ => panic!("not a trial game"),
        })
        .collect();
    indexes.sort();
    assert_eq!(indexes, (0..games.len() as u32).collect::<Vec<_>>());
    let dev = idea.version.unwrap();
    assert!(games.iter().all(|g| g.white.version.is_some() && g.black.version.is_some()));
    assert_eq!(games.iter().filter(|g| g.white.version == Some(dev)).count(), games.len() / 2);
    // The idea's version carries the deciding Elo.
    let v = lab.hub.store.version(dev).await.unwrap().unwrap();
    assert_eq!((v.line, v.elo), (Line::Idea { idea: idea.id }, short.elo));

    let kinds: Vec<EventKind> = std::iter::from_fn(|| sub.try_recv())
        .filter_map(|n| match n {
            BusNext::Event(e) => Some(e.kind),
            BusNext::Lagged(_) => None,
        })
        .collect();
    for kind in [EventKind::IdeaUpserted, EventKind::TrialUpserted, EventKind::TrialProgress, EventKind::GameUpserted] {
        assert!(kinds.contains(&kind), "no {kind:?}");
    }
}

#[tokio::test]
async fn ideas_are_dropped_by_the_first_stage_that_fails() {
    let lab = lab(2).await;
    // Unchanged bench: not a functional change.
    lab.repo.commit("comment", "main", &[("README.md", "a comment\n")], None);
    let same = lab.ideas.create(request("Comment", IdeaKind::Gain, "comment")).await.unwrap();
    let same = ended(&lab, same.id).await;
    assert_eq!(same.status, IdeaStatus::Dropped);
    assert!(same.verdict.as_deref().unwrap().starts_with("Dropped at the fingerprint: bench unchanged"));
    assert_eq!(
        same.trials.iter().map(|t| t.status).collect::<Vec<_>>(),
        [TrialStatus::Failed, TrialStatus::Skipped, TrialStatus::Skipped]
    );

    // Losing games: rejected at the screen.
    lab.repo.commit("worse", "main", &[("src/search.rs", "fn search() { worse(); }\n")], None);
    lab.fake.rounds(LOSSES);
    let worse = lab.ideas.create(request("Worse", IdeaKind::Gain, "worse")).await.unwrap();
    let worse = ended(&lab, worse.id).await;
    assert_eq!(worse.status, IdeaStatus::Dropped);
    assert!(
        worse.verdict.as_deref().unwrap().starts_with("Dropped at the screen: rejected [-5, 0]"),
        "{:?}",
        worse.verdict
    );

    // A stale Bench: line fails the fingerprint, naming the change.
    lab.repo.commit("stale", "main", &[("src/search.rs", "fn search() { stale(); }\n")], Some(7));
    let stale = lab.ideas.create(request("Stale", IdeaKind::Gain, "stale")).await.unwrap();
    let stale = ended(&lab, stale.id).await;
    assert!(stale.verdict.as_deref().unwrap().contains("the change benches"), "{:?}", stale.verdict);
}

#[tokio::test]
async fn a_speed_idea_needs_the_same_bench_and_no_slowdown() {
    let lab = lab(2).await;
    lab.repo.commit("faster", "main", &[("README.md", "faster\n")], None);
    let idea = lab.ideas.create(request("Faster", IdeaKind::Speed, "faster")).await.unwrap();
    let idea = ended(&lab, idea.id).await;
    assert_eq!(idea.status, IdeaStatus::Kept, "{:?}", idea.verdict);
    assert!(idea.verdict.as_deref().unwrap().starts_with("Kept: passed the speed, speed +0.00%"), "{:?}", idea.verdict);
    assert!(lab.fake.played().is_empty(), "no games");
}

#[tokio::test]
async fn a_custom_trial_plays_its_pairs_and_requests_are_checked() {
    let lab = lab(3).await;
    lab.repo.commit("test/comment", "main", &[("README.md", "x\n")], None);
    lab.fake.rounds(DRAWS);
    let mut req = request("False accepts", IdeaKind::Gain, "test/comment");
    req.custom = Some(CustomTrial { preset: Preset::Vstc, bounds: None, pairs: Some(20) });
    let idea = lab.ideas.create(req).await.unwrap();
    let idea = ended(&lab, idea.id).await;
    assert_eq!(idea.status, IdeaStatus::Kept);
    assert_eq!(idea.trials.len(), 1);
    assert_eq!(idea.trials[0].pairs, 20);
    // Three cores: 12 pairs, then the 8 left.
    assert_eq!(lab.fake.played().iter().map(|p| p.pairs).collect::<Vec<_>>(), [12, 8]);

    assert_eq!(lab.ideas.create(request("", IdeaKind::Gain, "x")).await.unwrap_err().status(), 400);
    assert_eq!(lab.ideas.create(request("No branch", IdeaKind::Gain, "nope")).await.unwrap_err().status(), 400);
    assert_eq!(lab.ideas.create(request("Main", IdeaKind::Gain, "main")).await.unwrap_err().status(), 400);
    // A batch is checked whole: nothing is queued when one is bad.
    let before = lab.ideas.list(IdeaFilter::default()).await.unwrap().len();
    let bad = BatchRequest {
        ideas: vec![request("Good", IdeaKind::Gain, "test/comment"), request("Bad", IdeaKind::Gain, "nope")],
        tag: "b1".into(),
    };
    assert!(lab.ideas.batch(bad).await.is_err());
    assert_eq!(lab.ideas.list(IdeaFilter::default()).await.unwrap().len(), before);
}

#[tokio::test]
async fn a_batch_is_tagged_and_runs_one_after_another() {
    let lab = lab(2).await;
    lab.repo.commit("a", "main", &[("src/search.rs", "fn search() { a(); }\n")], None);
    lab.repo.commit("b", "main", &[("src/search.rs", "fn search() { b(); }\n")], None);
    lab.fake.rounds(WINS);
    let ideas = lab
        .ideas
        .batch(BatchRequest {
            ideas: vec![request("A", IdeaKind::Gain, "a"), request("B", IdeaKind::Neutral, "b")],
            tag: "night-1".into(),
        })
        .await
        .unwrap();
    for i in &ideas {
        assert_eq!(ended(&lab, i.id).await.status, IdeaStatus::Kept);
    }
    let tagged = lab.ideas.list(IdeaFilter { tag: Some("night-1".into()), ..Default::default() }).await.unwrap();
    assert_eq!(tagged.len(), 2);
}

#[tokio::test]
async fn a_stopped_idea_resumes_with_the_rounds_it_played() {
    let lab = lab(2).await;
    lab.repo.commit("slow", "main", &[("src/search.rs", "fn search() { slow(); }\n")], None);
    // Even games hardly move the SPRT: the screen runs until stopped.
    lab.fake.rounds(EVEN);
    *lab.fake.round_delay.lock().unwrap() = Duration::from_millis(30);
    let idea = lab.ideas.create(request("Slow", IdeaKind::Gain, "slow")).await.unwrap();
    let screen = idea.trials[1].id;
    eventually("two rounds of the screen", || async {
        lab.ideas.trial(idea.id, screen).await.unwrap().penta.pairs() >= 16
    })
    .await;
    let stopped = lab.ideas.stop(idea.id).await.unwrap();
    assert!(matches!(stopped.status, IdeaStatus::Running | IdeaStatus::Stopped));
    let stopped = ended(&lab, idea.id).await;
    assert_eq!(stopped.status, IdeaStatus::Stopped);
    assert_eq!(stopped.verdict.as_deref(), Some("Stopped before it was decided"));
    let t = lab.ideas.trial(idea.id, screen).await.unwrap();
    assert_eq!(t.status, TrialStatus::Stopped);
    let pairs = t.penta.pairs();
    let games = lab.hub.store.count_trial_games(screen).await.unwrap();
    assert_eq!(u64::from(games), pairs * 2);

    // Resumed with winning rounds: the screen keeps its pairs and is decided, then the rest runs.
    lab.fake.rounds(WINS);
    *lab.fake.round_delay.lock().unwrap() = Duration::ZERO;
    let resumed = lab.ideas.resume(idea.id).await.unwrap();
    assert_eq!((resumed.status, resumed.verdict.clone(), resumed.ended_at.clone()), (IdeaStatus::Queued, None, None));
    let done = ended(&lab, idea.id).await;
    assert_eq!(done.status, IdeaStatus::Kept, "{:?}", done.verdict);
    let t = lab.ideas.trial(idea.id, screen).await.unwrap();
    assert!(t.penta.pairs() > pairs, "the screen kept its pairs: {:?}", t.penta);
    assert_eq!(lab.ideas.resume(idea.id).await.unwrap_err().status(), 409);
}

#[tokio::test]
async fn a_trial_without_fastchess_fails_the_idea_with_the_way_out() {
    let lab = lab(2).await;
    lab.fake.missing_tools.store(true, Ordering::SeqCst);
    lab.repo.commit("x", "main", &[("src/search.rs", "fn search() { x(); }\n")], None);
    let idea = lab.ideas.create(request("X", IdeaKind::Gain, "x")).await.unwrap();
    let idea = ended(&lab, idea.id).await;
    assert_eq!(idea.status, IdeaStatus::Failed);
    assert!(idea.verdict.as_deref().unwrap().contains("dameless setup"), "{:?}", idea.verdict);
    assert_eq!(idea.trials[1].status, TrialStatus::Failed);
    assert_eq!(idea.trials[2].status, TrialStatus::Skipped);
}

#[tokio::test]
async fn a_tune_writes_its_branch_and_goes_on_through_the_funnel() {
    let lab = lab(2).await;
    let tunables = format!(
        "{PARAMETERS}// tunables
"
    );
    lab.repo.commit("spsa/tunables", "main", &[("src/parameters.rs", &tunables)], None);
    lab.fake.rounds(WINS);
    let mut req = request("Tune search", IdeaKind::Tune, "spsa/tunables");
    // Big steps, so six iterations move the values.
    let big =
        |name: &str| dameless_core::spsa::TunableOverride { name: name.into(), r_end: Some(1.0), ..Default::default() };
    req.tune = Some(TuneSpec {
        iterations: 6,
        pairs: 2,
        params: vec![big("rfp_margin"), big("lmr_base")],
        ..TuneSpec::default()
    });
    let idea = lab.ideas.create(req).await.unwrap();
    assert_eq!(idea.version, None, "a tune's version comes from SPSA");
    assert_eq!(
        idea.trials.iter().map(|t| t.stage).collect::<Vec<_>>(),
        [Stage::Tune, Stage::Fingerprint, Stage::Screen, Stage::Confirm]
    );
    let idea = ended(&lab, idea.id).await;
    assert_eq!(idea.status, IdeaStatus::Kept, "{:?}", idea.verdict);
    assert!(idea.verdict.as_deref().unwrap().ends_with("Elo at confirmation"));

    // SPSA: one step per iteration, θ+ against θ− with the perturbed options.
    let view = lab.ideas.tune(idea.id).await.unwrap();
    assert_eq!(view.params, ["rfp_margin", "lmr_base"]);
    assert_eq!(view.steps.iter().map(|s| s.iteration).collect::<Vec<_>>(), [1, 2, 3, 4, 5, 6]);
    assert!(view.steps.iter().all(|s| s.result == 4 && s.values.len() == 2));
    let tune_rounds: Vec<_> = lab.fake.played().into_iter().filter(|p| !p.dev_options.is_empty()).collect();
    assert_eq!(tune_rounds.len(), 6);
    assert!(tune_rounds.iter().all(|p| p.pairs == 2 && p.dev_options.len() == 2));

    // The branch: new defaults, a Bench: line, the idea's version, built once.
    let branch = format!("tune/{}", idea.id);
    let message = lab.repo.git(&["log", "-1", "--format=%B", &branch]);
    assert!(message.contains("Apply SPSA tune") && message.contains("Bench: "), "{message}");
    let source = lab.repo.git(&["show", &format!("{branch}:src/parameters.rs")]);
    assert_ne!(source, PARAMETERS.trim_end());
    let v = lab.hub.store.version(idea.version.unwrap()).await.unwrap().unwrap();
    assert_eq!((v.name.as_str(), v.line), (branch.as_str(), Line::Idea { idea: idea.id }));
    assert_eq!(v.commit, lab.repo.git(&["rev-parse", &branch]));
    assert_eq!(v.bench, dameless_engine::git::bench_line(&message));
    let tune_trial = lab.ideas.trial(idea.id, idea.trials[0].id).await.unwrap();
    assert!(tune_trial.detail.as_deref().unwrap().contains("written to tune/"), "{:?}", tune_trial.detail);
}

#[tokio::test]
async fn a_kept_idea_is_promoted_to_champion() {
    let lab = lab(2).await;
    lab.repo.commit("better", "main", &[("src/search.rs", "fn search() { better(); }\n")], None);
    lab.fake.rounds(WINS);
    let idea = lab.ideas.create(request("Better", IdeaKind::Gain, "better")).await.unwrap();
    // Promoting before it is kept is refused.
    assert_eq!(lab.ideas.promote(idea.id).await.unwrap_err().status(), 409);
    let idea = ended(&lab, idea.id).await;
    assert_eq!(idea.status, IdeaStatus::Kept);
    let work = lab.ideas.promote(idea.id).await.unwrap();
    let work = lab.scheduler.wait(work.id).await.unwrap();
    assert_eq!(work.status, dameless_core::work::WorkStatus::Done, "{:?}", work.error);
    let champion = lab.ideas.versions().champion().await.unwrap();
    assert_eq!(champion.name, "v2");
    assert_eq!(champion.commit, lab.repo.git(&["rev-parse", "better"]));
    assert!(champion.elo.is_some());
    // The user's checkout, clean, was fast-forwarded.
    assert_eq!(lab.repo.git(&["rev-parse", "HEAD"]), champion.commit);

    // With uncommitted changes in the checkout, a promotion is refused and changes nothing.
    lab.repo.commit("more", "main", &[("src/search.rs", "fn search() { more(); }\n")], None);
    lab.fake.rounds(WINS);
    let more = lab.ideas.create(request("More", IdeaKind::Gain, "more")).await.unwrap();
    assert_eq!(ended(&lab, more.id).await.status, IdeaStatus::Kept);
    std::fs::write(lab.repo.path().join("src/search.rs"), "dirty\n").unwrap();
    let work = lab.ideas.promote(more.id).await.unwrap();
    let work = lab.scheduler.wait(work.id).await.unwrap();
    assert_eq!(work.status, dameless_core::work::WorkStatus::Failed);
    assert!(work.error.unwrap().contains("uncommitted"));
    assert_eq!(lab.ideas.versions().champion().await.unwrap().name, "v2");
}

#[tokio::test]
async fn a_stopped_tune_resumes_from_its_last_step() {
    let lab = lab(2).await;
    let tunables = format!("{PARAMETERS}// tunables\n");
    lab.repo.commit("spsa/tunables", "main", &[("src/parameters.rs", &tunables)], None);
    lab.fake.rounds(WINS);
    *lab.fake.round_delay.lock().unwrap() = Duration::from_millis(20);
    let big =
        |name: &str| dameless_core::spsa::TunableOverride { name: name.into(), r_end: Some(1.0), ..Default::default() };
    let mut req = request("Tune", IdeaKind::Tune, "spsa/tunables");
    req.tune = Some(TuneSpec {
        iterations: 30,
        pairs: 1,
        params: vec![big("rfp_margin"), big("lmr_base")],
        ..TuneSpec::default()
    });
    let idea = lab.ideas.create(req).await.unwrap();
    eventually("a few steps", || async { lab.hub.store.tune_steps(idea.id).await.unwrap().len() >= 3 }).await;
    lab.ideas.stop(idea.id).await.unwrap();
    assert_eq!(ended(&lab, idea.id).await.status, IdeaStatus::Stopped);
    let at = lab.hub.store.tune_steps(idea.id).await.unwrap().len();
    assert!(at < 30);

    *lab.fake.round_delay.lock().unwrap() = Duration::ZERO;
    lab.ideas.resume(idea.id).await.unwrap();
    let done = ended(&lab, idea.id).await;
    assert_eq!(done.status, IdeaStatus::Kept, "{:?}", done.verdict);
    let steps = lab.hub.store.tune_steps(idea.id).await.unwrap();
    assert_eq!(steps.iter().map(|s| s.iteration).collect::<Vec<_>>(), (1..=30).collect::<Vec<_>>());
}

#[tokio::test]
async fn a_branch_without_a_bench_line_is_refused_then_fixed_on_request() {
    let lab = lab(2).await;
    let sha = lab.repo.commit_without_bench("no-bench", "main", &[("src/search.rs", "fn search() { lmr(); }\n")]);
    let check = lab.ideas.check_branch("no-bench").await.unwrap();
    assert_eq!((check.status, check.commit.as_str(), check.bench_line), (BenchLineStatus::Missing, sha.as_str(), None));

    // Creating the idea is refused before anything is queued.
    let e = lab.ideas.create(request("No bench", IdeaKind::Gain, "no-bench")).await.unwrap_err();
    assert_eq!(e.code(), dameless_core::ErrorCode::Conflict);
    assert!(e.message().contains("has no Bench: line"), "{}", e.message());
    assert!(lab.ideas.list(IdeaFilter::default()).await.unwrap().is_empty());
    // A custom test has no fingerprint stage, so it is not held up.
    let custom = NewIdeaRequest {
        custom: Some(CustomTrial { preset: Preset::Vstc, bounds: None, pairs: Some(8) }),
        ..request("Test", IdeaKind::Gain, "no-bench")
    };
    lab.ideas.check(&custom).await.unwrap();

    // With fix_bench, the tip is built, reworded with its bench, and the idea runs on it.
    lab.fake.rounds(WINS);
    let tree = lab.repo.git(&["rev-parse", "no-bench^{tree}"]);
    let idea =
        lab.ideas.create(NewIdeaRequest { fix_bench: true, ..request("No bench", IdeaKind::Gain, "no-bench") }).await;
    let idea = idea.unwrap();
    let tip = lab.repo.git(&["rev-parse", "no-bench"]);
    assert_ne!(tip, sha);
    assert_eq!(lab.repo.git(&["rev-parse", "no-bench^{tree}"]), tree, "only the message changed");
    assert_eq!(lab.repo.git(&["rev-parse", "no-bench^"]), lab.repo.git(&["rev-parse", "main"]));
    let message = lab.repo.git(&["log", "-1", "--format=%B", "no-bench"]);
    assert!(message.starts_with("no-bench: change\n\nWhy it helps."), "{message}");
    let fixed = lab.ideas.check_branch("no-bench").await.unwrap();
    assert_eq!(fixed.status, BenchLineStatus::Ok);
    assert_eq!(fixed.bench_line, fixed.measured);
    assert_eq!(dameless_engine::git::bench_line(&message), fixed.measured);
    let idea = ended(&lab, idea.id).await;
    assert_eq!(idea.status, IdeaStatus::Kept, "{:?}", idea.verdict);
    // The reworded commit reused the build of the old one: one build of the branch, one of main.
    assert_eq!(lab.fake.builds.load(Ordering::SeqCst), 2);
    // The user's checkout is untouched.
    assert_eq!(lab.repo.git(&["symbolic-ref", "--short", "HEAD"]), "main");
    assert_eq!(lab.repo.git(&["status", "--porcelain"]), "");
}

#[tokio::test]
async fn a_stale_bench_line_is_found_once_built_and_fixed() {
    let lab = lab(2).await;
    lab.repo.commit("stale", "main", &[("src/search.rs", "fn search() { nmp(); }\n")], Some(123));
    let check = lab.ideas.check_branch("stale").await.unwrap();
    assert_eq!((check.status, check.bench_line, check.measured), (BenchLineStatus::Unbuilt, Some(123), None));
    // Unbuilt is not refused: only a build could tell, and the fingerprint will.
    lab.ideas.check(&request("Stale", IdeaKind::Gain, "stale")).await.unwrap();

    lab.ideas.versions().binary(&VersionRef::Name("stale".into()), &[]).await.unwrap();
    let check = lab.ideas.check_branch("stale").await.unwrap();
    assert_eq!(check.status, BenchLineStatus::Stale);
    let measured = check.measured.unwrap();
    assert_ne!(measured, 123);
    assert!(check.detail.contains("says Bench: 123"), "{}", check.detail);
    let e = lab.ideas.create(request("Stale", IdeaKind::Gain, "stale")).await.unwrap_err();
    assert_eq!(e.code(), dameless_core::ErrorCode::Conflict);
    assert!(e.message().contains("stale"));

    let fixed = lab.ideas.fix_bench_line("stale", Some(&check.commit)).await.unwrap();
    assert_eq!((fixed.status, fixed.bench_line, fixed.measured), (BenchLineStatus::Ok, Some(measured), Some(measured)));
    assert_ne!(fixed.commit, check.commit);
    let message = lab.repo.git(&["log", "-1", "--format=%B", "stale"]);
    assert_eq!(message.matches("Bench:").count(), 1, "{message}");
    // The version follows the reworded tip, with its bench.
    let v = lab.ideas.versions().resolve(&VersionRef::Name("stale".into())).await.unwrap();
    assert_eq!((v.commit.as_str(), v.bench), (fixed.commit.as_str(), Some(measured)));
    // Fixing again changes nothing.
    assert_eq!(lab.ideas.fix_bench_line("stale", None).await.unwrap().commit, fixed.commit);
}

#[tokio::test]
async fn a_branch_that_moved_is_not_reworded() {
    let lab = lab(2).await;
    let first = lab.repo.commit_without_bench("moving", "main", &[("src/search.rs", "fn search() { a(); }\n")]);
    let check = lab.ideas.check_branch("moving").await.unwrap();
    assert_eq!(check.commit, first);
    let second = lab.repo.commit_without_bench("moving", "moving", &[("src/search.rs", "fn search() { b(); }\n")]);
    let e = lab.ideas.fix_bench_line("moving", Some(&check.commit)).await.unwrap_err();
    assert_eq!(e.code(), dameless_core::ErrorCode::Conflict);
    assert!(e.message().contains("moved"), "{}", e.message());
    assert_eq!(lab.repo.git(&["rev-parse", "moving"]), second, "nothing was reworded");
    // main and upstream's branches are never reworded.
    assert_eq!(lab.ideas.fix_bench_line("main", None).await.unwrap_err().code(), dameless_core::ErrorCode::BadRequest);
    assert_eq!(lab.ideas.fix_bench_line("nope", None).await.unwrap_err().code(), dameless_core::ErrorCode::NotFound);
}
