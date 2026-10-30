//! Ideas and their trials: a hypothesis and a change, run through a funnel of stages, cheapest first. The first stage
//! that fails drops the idea with its evidence, so most bad ideas cost seconds or minutes and only survivors reach the
//! long games.
//!
//! The presets copy Reckless's own testing (its OpenBench instance): time controls, hash sizes and engine options.
//! The funnels are sized for one machine rather than a fleet: no VSTC screen (it passed about 90 % of ideas that do
//! nothing, at about the cost of the STC test it guarded), and SPRTs at alpha 0.05, beta 0.20 on [0, 4], so an idea
//! near 0 Elo is settled in about 20,000 games instead of about 76,000. The price is a few more small false accepts,
//! which the regression check after every fifth promotion is there to catch (see [`crate::regression`]). Clocks here
//! are as written; each trial scales its clock to the machine's speed before playing (see [`crate::scaling`]).

use serde::{Deserialize, Serialize};
use utoipa::ToSchema;

use crate::{
    ids::{IdeaId, Timestamp, TrialId, VersionId},
    scaling::Scaling,
    stats::{self, Pentanomial, Sprt, Verdict},
    time::TimeControl,
    version::{Estimate, Llr},
};

/// What kind of change an idea is, which picks its funnel.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum IdeaKind {
    /// Meant to gain Elo.
    Gain,
    /// A simplification or refactor that changes search: it must not lose Elo.
    Neutral,
    /// Must not change play at all (same bench) and must not be slower.
    Speed,
    /// SPSA moves the search constants, and the result is confirmed like a gain.
    Tune,
    /// A trained network: a run makes the net, the champion's code with it is tested like a gain.
    Net,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum IdeaStatus {
    Queued,
    Running,
    /// Every stage passed.
    Kept,
    /// A stage failed: see `verdict`.
    Dropped,
    /// Stopped by you or Claude before it was decided.
    Stopped,
    /// Something broke (a build, an engine), not the idea.
    Failed,
}

impl IdeaStatus {
    pub fn is_final(self) -> bool {
        !matches!(self, Self::Queued | Self::Running)
    }
}

/// Who had the idea.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum Author {
    #[default]
    You,
    Claude,
}

/// A hypothesis and a change to try.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, ToSchema)]
pub struct Idea {
    pub id: IdeaId,
    /// "Deeper LMR at high depth".
    pub title: String,
    pub hypothesis: String,
    pub kind: IdeaKind,
    pub author: Author,
    /// The change; for a tune, the branch with the tunables.
    pub branch: String,
    /// What it is tested against, usually the champion when queued.
    pub base: VersionId,
    /// The version under test (a tune's once SPSA has written it).
    pub version: Option<VersionId>,
    pub status: IdeaStatus,
    /// One sentence: "Kept: +3.1 ± 2.0 Elo at short games".
    pub verdict: Option<String>,
    pub trials: Vec<TrialSummary>,
    /// Batches are tags.
    pub tags: Vec<String>,
    /// Claude's or your reasoning, in markdown.
    pub notes: Option<String>,
    pub created_at: Timestamp,
    pub ended_at: Option<Timestamp>,
}

/// A stage of an idea.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum Stage {
    /// Bench both builds: a functional change must change the node count, a speed change must not.
    Fingerprint,
    /// A quick SPRT at VSTC that only throws out clear losses. No default funnel runs it any more; ideas queued before
    /// the single-machine funnels still have it.
    Screen,
    /// STC: the stage that decides.
    Short,
    /// LTC, when asked for.
    Long,
    /// Paired `bench` speed runs, no games.
    Speed,
    /// SPSA.
    Tune,
    /// A tune's STC at [0, 4].
    Confirm,
    /// A single trial with any preset and bounds, or a fixed number of pairs.
    Custom,
    /// A training run that makes a net (a `net` idea's first stage).
    Train,
    /// A regression check's games: the champion against the champion five promotions earlier, a fixed number of pairs.
    Regression,
}

impl Stage {
    /// How people read it: "short games".
    pub fn label(self) -> &'static str {
        match self {
            Self::Fingerprint => "fingerprint",
            Self::Screen => "quick screen",
            Self::Short => "short games",
            Self::Long => "long games",
            Self::Speed => "speed",
            Self::Tune => "tune",
            Self::Confirm => "confirmation",
            Self::Custom => "custom trial",
            Self::Train => "training",
            Self::Regression => "regression check",
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum TrialStatus {
    Waiting,
    Running,
    Passed,
    Failed,
    Stopped,
    /// Not run, because an earlier stage decided the idea.
    Skipped,
}

/// One stage of an idea: a set of games (or benches, or SPSA) and a verdict.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, ToSchema)]
pub struct Trial {
    pub id: TrialId,
    pub idea: IdeaId,
    pub stage: Stage,
    pub status: TrialStatus,
    pub preset: Option<Preset>,
    /// The clock as written (8+0.08).
    pub time: Option<TimeControl>,
    /// The clock after scaling (11+0.11).
    pub played: Option<TimeControl>,
    pub scaling: Option<Scaling>,
    /// SPRT bounds in normalized Elo, `[lower, upper]`.
    pub bounds: Option<[f64; 2]>,
    /// The SPRT's false-accept rate (with `bounds`).
    pub alpha: Option<f64>,
    /// The SPRT's false-reject rate (with `bounds`).
    pub beta: Option<f64>,
    /// A fixed number of game pairs instead of an SPRT (a custom trial).
    pub pairs: Option<u32>,
    /// Game pairs from the idea's side: `[LL, LD, DD+WL, WD, WW]`.
    pub penta: Pentanomial,
    /// Games won, drawn and lost by the idea's side.
    pub wdl: [u32; 3],
    pub llr: Option<Llr>,
    /// Logistic Elo ± 95 %.
    pub elo: Option<Estimate>,
    pub nelo: Option<Estimate>,
    /// One sentence for people.
    pub detail: Option<String>,
    pub started_at: Option<Timestamp>,
    pub ended_at: Option<Timestamp>,
}

impl Trial {
    /// Its statistics from its pairs: the LLR (with an SPRT), logistic and normalized Elo.
    pub fn statistics(penta: &Pentanomial, sprt: Option<Sprt>) -> (Option<Llr>, Option<Estimate>, Option<Estimate>) {
        let llr = sprt.map(|s| Llr { value: s.llr(penta), lower: s.lower(), upper: s.upper() });
        let est = stats::estimate(penta);
        (llr, est.map(|e| e.elo_interval()), est.map(|e| e.nelo_interval()))
    }

    /// What `Idea.trials` lists of it.
    pub fn summary(&self) -> TrialSummary {
        TrialSummary {
            id: self.id,
            stage: self.stage,
            status: self.status,
            preset: self.preset,
            bounds: self.bounds,
            alpha: self.alpha,
            beta: self.beta,
            pairs: self.penta.pairs(),
            llr: self.llr,
            elo: self.elo,
            detail: self.detail.clone(),
        }
    }
}

/// What an idea shows of each trial.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, ToSchema)]
pub struct TrialSummary {
    pub id: TrialId,
    pub stage: Stage,
    pub status: TrialStatus,
    pub preset: Option<Preset>,
    pub bounds: Option<[f64; 2]>,
    pub alpha: Option<f64>,
    pub beta: Option<f64>,
    /// Game pairs played.
    pub pairs: u64,
    pub llr: Option<Llr>,
    pub elo: Option<Estimate>,
    pub detail: Option<String>,
}

// ---------------------------------------------------------------------------------------------------------------------
// Presets
// ---------------------------------------------------------------------------------------------------------------------

/// Named game settings, as Reckless names its tests.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum Preset {
    /// 4+0.04: very short games, for a custom trial (the default funnels no longer screen with it).
    Vstc,
    /// 8+0.08, 16 MiB: the standard short test.
    Stc,
    /// 40+0.4, 64 MiB: the confirmation.
    Ltc,
    /// 8+0.08 with 2 MiB of hash, for changes to the transposition table.
    StcHash,
    /// 40+0.4 with 8 MiB of hash.
    LtcHash,
    /// 40+0.4 on 6 threads with 512 MiB, for changes to the parallel search.
    Smp,
    /// 25,000 nodes per move, for changes that should not alter speed.
    Nodes,
}

/// What a preset plays.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
pub struct PresetSettings {
    pub time: TimeControl,
    /// Search threads per engine.
    pub threads: u32,
    /// Hash table size in MiB.
    pub hash: u32,
}

impl Preset {
    pub const ALL: [Self; 7] = [Self::Vstc, Self::Stc, Self::Ltc, Self::StcHash, Self::LtcHash, Self::Smp, Self::Nodes];

    pub fn settings(self) -> PresetSettings {
        let clock = |initial_ms, increment_ms| TimeControl::Clock { initial_ms, increment_ms };
        let (time, threads, hash) = match self {
            Self::Vstc => (clock(4_000, 40), 1, 16),
            Self::Stc => (clock(8_000, 80), 1, 16),
            Self::Ltc => (clock(40_000, 400), 1, 64),
            Self::StcHash => (clock(8_000, 80), 1, 2),
            Self::LtcHash => (clock(40_000, 400), 1, 8),
            Self::Smp => (clock(40_000, 400), 6, 512),
            Self::Nodes => (TimeControl::Nodes { nodes: 25_000 }, 1, 16),
        };
        PresetSettings { time, threads, hash }
    }

    pub fn as_str(self) -> &'static str {
        match self {
            Self::Vstc => "vstc",
            Self::Stc => "stc",
            Self::Ltc => "ltc",
            Self::StcHash => "stc_hash",
            Self::LtcHash => "ltc_hash",
            Self::Smp => "smp",
            Self::Nodes => "nodes",
        }
    }

    /// Reads a preset by name; `stc-hash` (the old harness's spelling) works too.
    pub fn parse(name: &str) -> Option<Self> {
        let name = name.trim().replace('-', "_").to_ascii_lowercase();
        Self::ALL.into_iter().find(|p| p.as_str() == name)
    }
}

/// An SPRT's error rates: `alpha` is how often a change at the lower bound is accepted, `beta` how often one at the
/// upper bound is rejected.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize, ToSchema)]
pub struct ErrorRates {
    pub alpha: f64,
    pub beta: f64,
}

impl ErrorRates {
    /// Reckless's (OpenBench's) rates: what every SPRT here ran at before the single-machine funnels, so a stage stored
    /// without rates reads as these, and a custom trial runs at them unless told otherwise.
    pub const RECKLESS: Self = Self { alpha: 0.05, beta: 0.10 };
    /// The funnels' rates: a real gain at the upper bound is rejected 20 % of the time instead of 10 %, and in return a
    /// test is about half as long.
    pub const FUNNEL: Self = Self { alpha: 0.05, beta: 0.20 };

    /// What is wrong with these rates, if anything: each must lie strictly between 0 and 1, and together under 1.
    pub fn problem(self) -> Option<&'static str> {
        let open = |x: f64| x > 0.0 && x < 1.0;
        if !open(self.alpha) || !open(self.beta) {
            Some("SPRT error rates must lie strictly between 0 and 1")
        } else if self.alpha + self.beta >= 1.0 {
            Some("SPRT error rates must add up to less than 1")
        } else {
            None
        }
    }
}

fn reckless_alpha() -> f64 {
    ErrorRates::RECKLESS.alpha
}

fn reckless_beta() -> f64 {
    ErrorRates::RECKLESS.beta
}

/// The SPRT for bounds at these error rates.
pub fn sprt(bounds: [f64; 2], rates: ErrorRates) -> Sprt {
    Sprt { elo0: bounds[0], elo1: bounds[1], alpha: rates.alpha, beta: rates.beta }
}

// ---------------------------------------------------------------------------------------------------------------------
// Funnels
// ---------------------------------------------------------------------------------------------------------------------

/// The old screen's bounds: a wide non-regression SPRT that rejects a clearly harmful change in a few hundred pairs.
/// No default funnel runs it any more (a change that does nothing passed it about 90 % of the time, at about the cost
/// of the STC test it guarded); a custom trial can still ask for it.
pub const SCREEN_BOUNDS: [f64; 2] = [-5.0, 0.0];
/// A gain's bounds at STC and LTC, and a net's.
pub const GAIN_BOUNDS: [f64; 2] = [0.0, 4.0];
/// A custom trial's bounds when it names neither bounds nor pairs: OpenBench's gain bounds.
pub const CUSTOM_BOUNDS: [f64; 2] = [0.0, 3.0];
/// A neutral change's bounds at STC and LTC.
pub const NEUTRAL_BOUNDS: [f64; 2] = [-2.75, 0.25];
/// A tune's confirmation bounds, as Reckless confirms its tunes.
pub const TUNE_BOUNDS: [f64; 2] = [0.0, 4.0];
/// Paired bench runs in a speed stage.
pub const SPEED_RUNS: u32 = 5;

/// What the fingerprint stage wants of the two benches.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum FingerprintRule {
    /// A functional change: the bench must change.
    MustChange,
    /// A speed change: the bench must be identical.
    MustMatch,
}

/// How a stage is run and passed.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize, ToSchema)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum StageRule {
    Fingerprint {
        rule: FingerprintRule,
    },
    /// Games at a preset: until the SPRT at `bounds` (and `alpha`, `beta`) decides, or `pairs` pairs (then it passes
    /// when played). A stage stored before the rates were part of it reads as Reckless's 0.05 and 0.10, what it ran at.
    Games {
        preset: Preset,
        bounds: Option<[f64; 2]>,
        pairs: Option<u32>,
        #[serde(default = "reckless_alpha")]
        alpha: f64,
        #[serde(default = "reckless_beta")]
        beta: f64,
    },
    /// Paired bench runs: dev fails when measurably slower.
    Speed {
        runs: u32,
    },
    /// SPSA writes the change; it passes when its iterations are done and the values are written.
    Tune,
    /// A training run makes a net; it passes when the net is registered and the idea's version (the champion's code
    /// with it) is recorded.
    Train,
}

/// One stage of a funnel.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize, ToSchema)]
pub struct StagePlan {
    pub stage: Stage,
    pub rule: StageRule,
}

impl StageRule {
    /// The SPRT a games stage runs, if it runs one.
    pub fn sprt(&self) -> Option<Sprt> {
        match *self {
            Self::Games { bounds: Some(b), alpha, beta, .. } => Some(sprt(b, ErrorRates { alpha, beta })),
            _ => None,
        }
    }
}

impl StagePlan {
    fn games(stage: Stage, preset: Preset, bounds: [f64; 2]) -> Self {
        let ErrorRates { alpha, beta } = ErrorRates::FUNNEL;
        Self { stage, rule: StageRule::Games { preset, bounds: Some(bounds), pairs: None, alpha, beta } }
    }

    fn fingerprint(rule: FingerprintRule) -> Self {
        Self { stage: Stage::Fingerprint, rule: StageRule::Fingerprint { rule } }
    }
}

/// The funnel for a kind of idea, as the design's table has it; `long` adds the LTC stage to those that have one.
/// Every SPRT runs at alpha 0.05, beta 0.20 ([`ErrorRates::FUNNEL`]).
///
/// | kind    | stages                                                                       |
/// | ------- | ---------------------------------------------------------------------------- |
/// | gain    | fingerprint (must change) → short STC [0, 4] → long LTC [0, 4]               |
/// | neutral | fingerprint (must change) → short STC [-2.75, 0.25] → long LTC [-2.75, 0.25] |
/// | speed   | fingerprint (must match) → speed (5 paired runs)                             |
/// | tune    | tune → fingerprint (must change) → confirm STC [0, 4] → long LTC [0, 4]      |
/// | net     | train → fingerprint (must change) → short STC [0, 4] → long LTC [0, 4]       |
///
/// An idea stores its funnel when it is queued, so one queued or running when these change keeps the rules it began
/// with.
pub fn funnel(kind: IdeaKind, long: bool) -> Vec<StagePlan> {
    let functional = |decide: Stage, bounds: [f64; 2]| {
        let mut stages =
            vec![StagePlan::fingerprint(FingerprintRule::MustChange), StagePlan::games(decide, Preset::Stc, bounds)];
        if long {
            stages.push(StagePlan::games(Stage::Long, Preset::Ltc, bounds));
        }
        stages
    };
    match kind {
        IdeaKind::Gain => functional(Stage::Short, GAIN_BOUNDS),
        IdeaKind::Neutral => functional(Stage::Short, NEUTRAL_BOUNDS),
        IdeaKind::Speed => vec![
            StagePlan::fingerprint(FingerprintRule::MustMatch),
            StagePlan { stage: Stage::Speed, rule: StageRule::Speed { runs: SPEED_RUNS } },
        ],
        IdeaKind::Tune => {
            let mut stages = vec![StagePlan { stage: Stage::Tune, rule: StageRule::Tune }];
            stages.extend(functional(Stage::Confirm, TUNE_BOUNDS));
            stages
        }
        IdeaKind::Net => {
            let mut stages = vec![StagePlan { stage: Stage::Train, rule: StageRule::Train }];
            stages.extend(functional(Stage::Short, GAIN_BOUNDS));
            stages
        }
    }
}

/// A custom idea: one trial at `preset`, with SPRT `bounds` or a fixed number of `pairs` (with neither, OpenBench's
/// [0, 3]), at `rates` (Reckless's 0.05 and 0.10 when left out). What `chess test` was, including the false-accept
/// check.
pub fn custom(
    preset: Preset,
    bounds: Option<[f64; 2]>,
    pairs: Option<u32>,
    rates: Option<ErrorRates>,
) -> Vec<StagePlan> {
    let bounds = match (bounds, pairs) {
        (Some(b), _) => Some(b),
        (None, Some(_)) => None,
        (None, None) => Some(CUSTOM_BOUNDS),
    };
    let ErrorRates { alpha, beta } = rates.unwrap_or(ErrorRates::RECKLESS);
    vec![StagePlan { stage: Stage::Custom, rule: StageRule::Games { preset, bounds, pairs, alpha, beta } }]
}

/// What happens after a stage of a funnel ends.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Next {
    /// Run the stage at this index of the funnel.
    Run(usize),
    /// Every stage passed. A custom idea is kept when its one trial passed.
    Kept,
    /// The stage failed.
    Dropped,
}

/// The step after stage `index` of `funnel` ended, passed or not.
pub fn next(funnel: &[StagePlan], index: usize, passed: bool) -> Next {
    if !passed {
        Next::Dropped
    } else if index + 1 < funnel.len() {
        Next::Run(index + 1)
    } else {
        Next::Kept
    }
}

// ---------------------------------------------------------------------------------------------------------------------
// Verdicts
// ---------------------------------------------------------------------------------------------------------------------

/// How a stage came out: passed or not, and one sentence with the numbers.
#[derive(Clone, Debug, PartialEq)]
pub struct StageVerdict {
    pub passed: bool,
    pub detail: String,
}

/// A build's bench as the fingerprint reads it: what it measured, and its commit's `Bench:` line.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Bench {
    pub measured: u64,
    pub expected: Option<u64>,
}

/// The fingerprint stage. A build whose bench differs from its commit's `Bench:` line is suspect (a stale line, or
/// search that is not deterministic), as Reckless's CI would say: testing it would test something other than the
/// commit claims, so that fails first, for either build.
pub fn fingerprint(rule: FingerprintRule, base: Bench, dev: Bench) -> StageVerdict {
    for (side, build) in [("base", base), ("the change", dev)] {
        if let Some(expected) = build.expected.filter(|e| *e != build.measured) {
            let detail = format!(
                "{side} benches {} but its commit says Bench: {expected}: fix the Bench: line, or find the non-determinism",
                build.measured
            );
            return StageVerdict { passed: false, detail };
        }
    }
    let (b, d) = (base.measured, dev.measured);
    let (passed, detail) = match (rule, b == d) {
        (FingerprintRule::MustChange, false) => (true, format!("bench {b} → {d}")),
        (FingerprintRule::MustChange, true) => {
            (false, format!("bench unchanged at {b}: the change does not alter search (a speed idea?)"))
        }
        (FingerprintRule::MustMatch, true) => (true, format!("bench unchanged at {b}")),
        (FingerprintRule::MustMatch, false) => (false, format!("bench changed {b} → {d}: the change alters search")),
    };
    StageVerdict { passed, detail }
}

/// The speed stage from paired runs: each `(base_nps, dev_nps)` benched back to back (in alternating order, so drift
/// in the machine cancels out). Dev fails when it is slower by more than 1 % and by more than twice the standard
/// error.
pub fn speed(runs: &[(f64, f64)]) -> StageVerdict {
    let ratios: Vec<f64> = runs.iter().map(|(b, d)| (d / b - 1.0) * 100.0).collect();
    let (diff, se) = mean_and_error(&ratios);
    let passed = !(diff < -1.0 && diff < -2.0 * se);
    let detail = format!("speed {diff:+.2}% ± {:.2}% over {} paired runs", 2.0 * se, ratios.len());
    StageVerdict { passed, detail }
}

/// The mean and its standard error (zero error with fewer than two values).
fn mean_and_error(v: &[f64]) -> (f64, f64) {
    if v.is_empty() {
        return (0.0, 0.0);
    }
    let n = v.len() as f64;
    let mean = v.iter().sum::<f64>() / n;
    if v.len() < 2 {
        return (mean, 0.0);
    }
    let var = v.iter().map(|x| (x - mean).powi(2)).sum::<f64>() / (n - 1.0);
    (mean, (var / n).sqrt())
}

/// A games stage once its games are over: with an SPRT it passes when accepted, without one when its pairs were
/// played. `scaled` is the clock it was played at and the factor, for the sentence.
pub fn games(penta: &Pentanomial, sprt: Option<Sprt>, scaled: Option<(TimeControl, f64)>) -> StageVerdict {
    let est = stats::estimate(penta)
        .map(|e| format!("{:+.1} ± {:.1} Elo ({:+.1} nElo)", e.elo, e.elo_error, e.nelo))
        .unwrap_or_default();
    let games = penta.pairs() * 2;
    let (passed, mut detail) = match sprt {
        Some(s) => {
            let b = format!("[{}, {}]", s.elo0, s.elo1);
            match s.verdict(s.llr(penta)) {
                Verdict::Accepted => (true, format!("accepted {b} after {games} games, {est}")),
                Verdict::Rejected => (false, format!("rejected {b} after {games} games, {est}")),
                Verdict::Continue => (false, format!("undecided after {games} games, {est}")),
            }
        }
        None => (true, format!("{games} games, {est}")),
    };
    if let Some((played, scale)) = scaled {
        detail += &format!(", at {} (×{scale:.2})", played.seconds_text());
    }
    StageVerdict { passed, detail }
}

/// The idea's one sentence once it ends: "Kept: +3.1 ± 2.0 Elo at short games", "Dropped at the short games: rejected
/// [0, 4] after 9,412 games, …".
pub fn idea_verdict(status: IdeaStatus, deciding: Option<(Stage, Option<Estimate>)>, detail: Option<&str>) -> String {
    match (status, deciding) {
        (IdeaStatus::Kept, Some((stage, Some(elo)))) => format!("Kept: {} Elo at {}", elo.text(), stage.label()),
        (IdeaStatus::Kept, Some((stage, None))) => {
            format!("Kept: passed the {}{}", stage.label(), detail.map(|d| format!(", {d}")).unwrap_or_default())
        }
        (IdeaStatus::Dropped, Some((stage, _))) => {
            format!("Dropped at the {}{}", stage.label(), detail.map(|d| format!(": {d}")).unwrap_or_default())
        }
        (IdeaStatus::Stopped, _) => "Stopped before it was decided".into(),
        (IdeaStatus::Failed, _) => format!("Failed{}", detail.map(|d| format!(": {d}")).unwrap_or_default()),
        (status, _) => format!("{status:?}"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn stages(f: &[StagePlan]) -> Vec<Stage> {
        f.iter().map(|s| s.stage).collect()
    }

    /// A games stage at the funnels' rates.
    fn fast(preset: Preset, bounds: [f64; 2]) -> StageRule {
        StageRule::Games { preset, bounds: Some(bounds), pairs: None, alpha: 0.05, beta: 0.20 }
    }

    #[test]
    fn funnels_start_cheap_as_the_design_has_them() {
        assert_eq!(stages(&funnel(IdeaKind::Gain, true)), [Stage::Fingerprint, Stage::Short, Stage::Long]);
        let gain = funnel(IdeaKind::Gain, false);
        assert_eq!(stages(&gain), [Stage::Fingerprint, Stage::Short]);
        assert_eq!(gain[1].rule, fast(Preset::Stc, [0.0, 4.0]));
        assert_eq!(funnel(IdeaKind::Gain, true)[2].rule, fast(Preset::Ltc, [0.0, 4.0]));
        assert_eq!(stages(&funnel(IdeaKind::Speed, true)), [Stage::Fingerprint, Stage::Speed]);
        assert_eq!(
            stages(&funnel(IdeaKind::Tune, true)),
            [Stage::Tune, Stage::Fingerprint, Stage::Confirm, Stage::Long]
        );
        let neutral = funnel(IdeaKind::Neutral, true);
        assert_eq!(neutral[0].rule, StageRule::Fingerprint { rule: FingerprintRule::MustChange });
        assert_eq!(neutral[1].rule, fast(Preset::Stc, [-2.75, 0.25]));
        assert_eq!(neutral[2].rule, fast(Preset::Ltc, [-2.75, 0.25]));
        assert_eq!(stages(&funnel(IdeaKind::Net, true)), [Stage::Train, Stage::Fingerprint, Stage::Short, Stage::Long]);
        let net = funnel(IdeaKind::Net, false);
        assert_eq!(net[2].rule, fast(Preset::Stc, [0.0, 4.0]));
        assert_eq!(next(&net, 0, true), Next::Run(1));
        assert_eq!(next(&net, 2, true), Next::Kept);
        let tune = funnel(IdeaKind::Tune, true);
        assert_eq!(tune[2].rule, fast(Preset::Stc, [0.0, 4.0]));
        assert_eq!(tune[3].rule, fast(Preset::Ltc, [0.0, 4.0]));
        assert_eq!(funnel(IdeaKind::Speed, false)[0].rule, StageRule::Fingerprint { rule: FingerprintRule::MustMatch });
        // No default funnel screens any more.
        for kind in [IdeaKind::Gain, IdeaKind::Neutral, IdeaKind::Speed, IdeaKind::Tune, IdeaKind::Net] {
            assert!(!stages(&funnel(kind, true)).contains(&Stage::Screen), "{kind:?}");
        }
    }

    #[test]
    fn a_stored_stage_without_rates_keeps_the_rates_it_ran_at() {
        let old: StagePlan = serde_json::from_value(serde_json::json!({
            "stage": "screen",
            "rule": { "type": "games", "preset": "vstc", "bounds": [-5.0, 0.0], "pairs": null }
        }))
        .unwrap();
        let sprt = old.rule.sprt().unwrap();
        assert_eq!((sprt.alpha, sprt.beta), (0.05, 0.10));
        let new = serde_json::to_value(funnel(IdeaKind::Gain, false)[1]).unwrap();
        assert_eq!((new["rule"]["alpha"].as_f64(), new["rule"]["beta"].as_f64()), (Some(0.05), Some(0.20)));
        assert_eq!(StageRule::Tune.sprt(), None);
    }

    #[test]
    fn error_rates_are_checked() {
        assert_eq!(ErrorRates::FUNNEL.problem(), None);
        assert!(ErrorRates { alpha: 0.0, beta: 0.2 }.problem().is_some());
        assert!(ErrorRates { alpha: 0.05, beta: 1.0 }.problem().is_some());
        assert!(ErrorRates { alpha: 0.6, beta: 0.5 }.problem().is_some());
    }

    #[test]
    fn custom_ideas_are_one_trial() {
        let c = custom(Preset::Vstc, None, None, None);
        assert_eq!(
            c,
            [StagePlan {
                stage: Stage::Custom,
                rule: StageRule::Games {
                    preset: Preset::Vstc,
                    bounds: Some([0.0, 3.0]),
                    pairs: None,
                    alpha: 0.05,
                    beta: 0.10
                }
            }]
        );
        let fixed = custom(Preset::Vstc, None, Some(2000), None);
        assert_eq!(
            fixed[0].rule,
            StageRule::Games { preset: Preset::Vstc, bounds: None, pairs: Some(2000), alpha: 0.05, beta: 0.10 }
        );
        let rates = custom(Preset::Stc, Some([0.0, 4.0]), None, Some(ErrorRates { alpha: 0.025, beta: 0.2 }));
        assert_eq!(
            rates[0].rule,
            StageRule::Games { preset: Preset::Stc, bounds: Some([0.0, 4.0]), pairs: None, alpha: 0.025, beta: 0.2 }
        );
        assert_eq!(next(&c, 0, true), Next::Kept);
        assert_eq!(next(&c, 0, false), Next::Dropped);
    }

    #[test]
    fn the_next_stage_follows_a_pass_and_a_failure_drops() {
        let f = funnel(IdeaKind::Gain, true);
        assert_eq!(next(&f, 0, true), Next::Run(1));
        assert_eq!(next(&f, 1, true), Next::Run(2));
        assert_eq!(next(&f, 2, true), Next::Kept);
        assert_eq!(next(&f, 1, false), Next::Dropped);
    }

    #[test]
    fn presets_copy_reckless() {
        assert_eq!(Preset::Stc.settings().time.seconds_text(), "8+0.08");
        assert_eq!(Preset::Vstc.settings().time.seconds_text(), "4+0.04");
        assert_eq!(
            Preset::Ltc.settings(),
            PresetSettings { time: TimeControl::clock(40_000, 400), threads: 1, hash: 64 }
        );
        assert_eq!(Preset::Smp.settings().threads, 6);
        assert_eq!(Preset::StcHash.settings().hash, 2);
        assert_eq!(Preset::LtcHash.settings().hash, 8);
        assert_eq!(Preset::Nodes.settings().time, TimeControl::Nodes { nodes: 25_000 });
        assert_eq!(Preset::parse("stc-hash"), Some(Preset::StcHash));
        assert_eq!(serde_json::to_value(Preset::LtcHash).unwrap(), "ltc_hash");
        assert_eq!(sprt([0.0, 4.0], ErrorRates::FUNNEL), Sprt { elo0: 0.0, elo1: 4.0, alpha: 0.05, beta: 0.20 });
    }

    #[test]
    fn the_fingerprint_wants_a_change_only_for_functional_changes() {
        let (main, same, other) = (
            Bench { measured: 100, expected: Some(100) },
            Bench { measured: 100, expected: None },
            Bench { measured: 200, expected: Some(200) },
        );
        assert!(fingerprint(FingerprintRule::MustChange, main, other).passed);
        assert!(!fingerprint(FingerprintRule::MustChange, main, same).passed);
        assert!(fingerprint(FingerprintRule::MustMatch, main, same).passed);
        assert!(!fingerprint(FingerprintRule::MustMatch, main, other).passed);
    }

    #[test]
    fn the_fingerprint_fails_a_build_that_disagrees_with_its_bench_line() {
        let main = Bench { measured: 100, expected: Some(100) };
        let r = fingerprint(FingerprintRule::MustChange, main, Bench { measured: 200, expected: Some(150) });
        assert!(
            !r.passed && r.detail.contains("the change benches 200") && r.detail.contains("Bench: 150"),
            "{}",
            r.detail
        );
        let r = fingerprint(FingerprintRule::MustMatch, Bench { measured: 100, expected: Some(90) }, main);
        assert!(!r.passed && r.detail.starts_with("base"), "{}", r.detail);
    }

    #[test]
    fn speed_fails_only_a_measurably_slower_change() {
        assert!(speed(&[(100.0, 100.0), (100.0, 101.0), (100.0, 99.5)]).passed);
        let slow = speed(&[(100.0, 95.0), (100.0, 96.0), (100.0, 95.5)]);
        assert!(!slow.passed && slow.detail.contains("over 3 paired runs"), "{}", slow.detail);
        // Slower by more than 1 % but within the noise: passes.
        assert!(speed(&[(100.0, 90.0), (100.0, 110.0), (100.0, 97.0)]).passed);
    }

    #[test]
    fn games_pass_on_an_accept_or_when_their_pairs_are_played() {
        let gain = Some(sprt(GAIN_BOUNDS, ErrorRates::FUNNEL));
        let won = Pentanomial([0, 0, 0, 0, 400]);
        let v = games(&won, gain, Some((TimeControl::clock(11_000, 110), 1.375)));
        assert!(v.passed && v.detail.starts_with("accepted [0, 4] after 800 games"), "{}", v.detail);
        assert!(v.detail.ends_with(", at 11+0.11 (×1.38)"), "{}", v.detail);
        let lost = games(&Pentanomial([400, 0, 0, 0, 0]), Some(sprt(SCREEN_BOUNDS, ErrorRates::RECKLESS)), None);
        assert!(!lost.passed && lost.detail.starts_with("rejected [-5, 0]"), "{}", lost.detail);
        let open = games(&Pentanomial([1, 2, 3, 2, 1]), gain, None);
        assert!(!open.passed && open.detail.starts_with("undecided after 18 games"));
        assert!(games(&Pentanomial([1, 2, 3, 2, 1]), None, None).passed);
    }

    #[test]
    fn trials_compute_their_statistics() {
        let penta = Pentanomial([50, 300, 1000, 500, 150]);
        let (llr, elo, nelo) = Trial::statistics(&penta, Some(sprt(CUSTOM_BOUNDS, ErrorRates::RECKLESS)));
        let llr = llr.unwrap();
        assert!((llr.lower - -2.25).abs() < 0.01 && (llr.upper - 2.89).abs() < 0.01);
        assert!(elo.unwrap().elo > 0.0 && nelo.unwrap().elo > 0.0);
        // At beta 0.20 the bounds are ln(0.2 / 0.95) and ln(0.8 / 0.05).
        let (llr, _, _) = Trial::statistics(&penta, Some(sprt(GAIN_BOUNDS, ErrorRates::FUNNEL)));
        let llr = llr.unwrap();
        assert!((llr.lower - (0.2f64 / 0.95).ln()).abs() < 1e-12 && (llr.upper - 16f64.ln()).abs() < 1e-12);
        assert_eq!(Trial::statistics(&Pentanomial::default(), None), (None, None, None));
    }

    #[test]
    fn verdicts_read_as_one_sentence() {
        let e = Estimate::plus_minus(3.1, 2.0);
        assert_eq!(
            idea_verdict(IdeaStatus::Kept, Some((Stage::Short, Some(e))), None),
            "Kept: +3.1 ± 2.0 Elo at short games"
        );
        assert_eq!(
            idea_verdict(IdeaStatus::Dropped, Some((Stage::Short, None)), Some("rejected [0, 4] after 9412 games")),
            "Dropped at the short games: rejected [0, 4] after 9412 games"
        );
    }
}
