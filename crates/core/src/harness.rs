//! Checking the harness: does this machine's testing reject a change that does nothing?
//!
//! The check plays the champion against a branch identical to it (only a comment differs, so the bench is the same)
//! at VSTC with the gain's bounds [0, 3]. A sound harness rejects it almost always (an SPRT at [0, 3] accepts a
//! change that does nothing at most 5 % of the time), and its Elo interval contains 0. An accept, or an estimate
//! clearly away from 0, points at a gross bug (colour or order bias, dev and base mixed up, a broken book) and means
//! no result on this machine can be trusted until it is investigated. An optional fixed run of game pairs adds a
//! symmetry check: with identical engines, pairs lost twice should be about as many as pairs won twice (LL ≈ WW), and
//! lost-and-drawn about as many as won-and-drawn (LD ≈ WD).
//!
//! The result belongs to the machine and the tools it was played with: a check on another CPU, or with another
//! fastchess or book, says nothing about this one, so the status calls it out of date.

use serde::{Deserialize, Serialize};
use utoipa::ToSchema;

use crate::{
    ideas::{GAIN_BOUNDS, Preset, sprt},
    ids::{IdeaId, Timestamp},
    stats::{self, Pentanomial, Verdict},
    version::Estimate,
};

/// The tag every harness check idea carries.
pub const TAG: &str = "harness";
/// The branch the app makes for the check: `main`'s tip plus a comment.
pub const BRANCH: &str = "harness/identical";
/// The check's preset: quick games, as the old harness's false-accept check.
pub const PRESET: Preset = Preset::Vstc;
/// The check's bounds: the gain's, which a change that does nothing passes at most 5 % of the time.
pub const BOUNDS: [f64; 2] = GAIN_BOUNDS;
/// The symmetry run's usual length, in game pairs.
pub const SYMMETRY_PAIRS: u32 = 2_000;
/// How far apart, in standard errors, mirrored pentanomial counts may be before the pairs are called lopsided. Loose
/// on purpose: this catches gross bias, and a false alarm costs a needless investigation.
pub const SYMMETRY_Z: f64 = 3.0;

/// The machine a check ran on.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
pub struct Machine {
    /// The CPU's brand string, "Intel(R) Core(TM) i7-10700K CPU @ 3.80GHz".
    pub cpu: String,
    /// Physical cores.
    pub cores: u32,
    /// "Windows 11 Home 24H2", "Linux 6.8 Ubuntu 24.04".
    pub os: String,
}

/// What a check is valid for: the machine and the tools that play the games.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
pub struct Environment {
    pub machine: Machine,
    /// The installed fastchess, by its pinned name ("fastchess-1.8.2"); none when not installed.
    pub fastchess: Option<String>,
    /// The opening book's file name.
    pub book: Option<String>,
}

/// How symmetric the pairs of identical engines came out.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, ToSchema)]
pub struct Symmetry {
    pub passed: bool,
    pub pairs: u64,
    /// `(WW − LL) / √(WW + LL)`: how many standard errors the doubles lean towards the change.
    pub z_doubles: f64,
    /// `(WD − LD) / √(WD + LD)`, the same for the half points.
    pub z_halves: f64,
    /// One sentence for people.
    pub detail: String,
}

/// The symmetry verdict of a run between identical engines.
pub fn symmetry(penta: &Pentanomial) -> Symmetry {
    let [ll, ld, _, wd, ww] = penta.0;
    let z = |low: u64, high: u64| {
        let n = (low + high) as f64;
        if n == 0.0 { 0.0 } else { (high as f64 - low as f64) / n.sqrt() }
    };
    let (z_doubles, z_halves) = (z(ll, ww), z(ld, wd));
    let passed = z_doubles.abs() <= SYMMETRY_Z && z_halves.abs() <= SYMMETRY_Z;
    let counts = format!("LL {ll} / WW {ww}, LD {ld} / WD {wd}");
    let detail = if passed {
        format!("Symmetric over {} pairs ({counts})", penta.pairs())
    } else {
        format!("Lopsided over {} pairs ({counts}): a colour or order bias?", penta.pairs())
    };
    Symmetry { passed, pairs: penta.pairs(), z_doubles, z_halves, detail }
}

/// The verdict of the check's SPRT trial (and the symmetry run, when there was one): passed when the SPRT rejected
/// the identical engine, its Elo interval contains 0, and the pairs were symmetric. The summary is one sentence.
pub fn judge(penta: &Pentanomial, bounds: [f64; 2], symmetry: Option<&Symmetry>) -> (bool, Option<Estimate>, String) {
    let s = sprt(bounds);
    let verdict = s.verdict(s.llr(penta));
    let elo = stats::estimate(penta).map(|e| e.elo_interval());
    let games = penta.pairs() * 2;
    let elo_text = elo.map(|e| format!("{} Elo", e.text())).unwrap_or_else(|| "no estimate".into());
    let contains_zero = elo.is_some_and(|e| e.low <= 0.0 && e.high >= 0.0);
    let (passed, summary) = match verdict {
        Verdict::Accepted => (
            false,
            format!(
                "An identical engine was accepted ({elo_text} after {games} games): do not trust results until this is \
                 investigated"
            ),
        ),
        Verdict::Continue => (false, format!("The check did not finish: undecided after {games} games, {elo_text}")),
        Verdict::Rejected if !contains_zero => (
            false,
            format!(
                "An identical engine measured {elo_text} after {games} games, an interval without 0: do not trust \
                 results until this is investigated"
            ),
        ),
        Verdict::Rejected => (true, format!("An identical engine was rejected: {elo_text} after {games} games")),
    };
    match symmetry {
        Some(sym) if !sym.passed => {
            (false, elo, format!("{summary}. {}: do not trust results until this is investigated", sym.detail))
        }
        Some(sym) => (passed, elo, format!("{summary}. {}", sym.detail)),
        None => (passed, elo, summary),
    }
}

/// A finished check, as stored for the machine (`meta` key `harness.check`).
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, ToSchema)]
pub struct HarnessCheck {
    pub machine: Machine,
    pub at: Timestamp,
    /// The idea that ran it.
    pub idea: IdeaId,
    pub passed: bool,
    /// Logistic Elo of the identical engine ± 95 %.
    pub elo: Option<Estimate>,
    pub symmetry: Option<Symmetry>,
    /// The champion's commit it was played with.
    pub engine_commit: String,
    pub fastchess: Option<String>,
    pub book: Option<String>,
    /// One sentence for people.
    pub summary: String,
}

impl HarnessCheck {
    /// The environment it certifies.
    pub fn environment(&self) -> Environment {
        Environment { machine: self.machine.clone(), fastchess: self.fastchess.clone(), book: self.book.clone() }
    }
}

/// `GET /api/harness`: whether results on this machine can be trusted. Trials' verdicts show `summary` beside them
/// while `trusted` is false.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, ToSchema)]
pub struct HarnessStatus {
    /// A check has finished on some machine.
    pub checked: bool,
    /// The last check was on this machine, with the fastchess and book installed now.
    pub current: bool,
    /// The last check's outcome.
    pub passed: Option<bool>,
    pub at: Option<Timestamp>,
    pub idea: Option<IdeaId>,
    /// Checked here, current and passed: results on this machine can be trusted.
    pub trusted: bool,
    /// A check queued or running now.
    pub running: Option<IdeaId>,
    /// One sentence for people.
    pub summary: String,
    /// The last check, in full.
    pub last: Option<HarnessCheck>,
}

/// The status from the stored check, the environment now, and a check in progress.
pub fn status(last: Option<HarnessCheck>, now: &Environment, running: Option<IdeaId>) -> HarnessStatus {
    let Some(check) = last else {
        let summary = match running {
            Some(_) => "The harness is being checked on this machine".to_owned(),
            None => "This machine's harness is unchecked: check it before trusting results".to_owned(),
        };
        return HarnessStatus {
            checked: false,
            current: false,
            passed: None,
            at: None,
            idea: None,
            trusted: false,
            running,
            summary,
            last: None,
        };
    };
    let then = check.environment();
    let current = then == *now;
    let summary = if then.machine != now.machine {
        "The harness was checked on another machine: check it again on this one".to_owned()
    } else if !current {
        "The harness was checked with another fastchess or opening book: check it again".to_owned()
    } else if check.passed {
        format!("Harness checked {}: {}", check.at.as_str().get(..10).unwrap_or(check.at.as_str()), check.summary)
    } else {
        format!("The harness check failed: {}", check.summary)
    };
    HarnessStatus {
        checked: true,
        current,
        passed: Some(check.passed),
        at: Some(check.at.clone()),
        idea: Some(check.idea),
        trusted: current && check.passed,
        running,
        summary,
        last: Some(check),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn machine(cpu: &str) -> Machine {
        Machine { cpu: cpu.into(), cores: 8, os: "Windows 11".into() }
    }

    fn env(cpu: &str, fastchess: &str) -> Environment {
        Environment { machine: machine(cpu), fastchess: Some(fastchess.into()), book: Some("UHO.epd".into()) }
    }

    fn check(passed: bool) -> HarnessCheck {
        HarnessCheck {
            machine: machine("i7"),
            at: Timestamp("2026-10-26T10:00:00.000Z".into()),
            idea: IdeaId(4),
            passed,
            elo: None,
            symmetry: None,
            engine_commit: "abc".into(),
            fastchess: Some("fastchess-1.8.2".into()),
            book: Some("UHO.epd".into()),
            summary: "An identical engine was rejected".into(),
        }
    }

    #[test]
    fn an_even_match_is_rejected_and_passes() {
        let even = Pentanomial([4000, 8000, 16000, 8000, 4000]);
        let (passed, elo, summary) = judge(&even, BOUNDS, None);
        assert!(passed, "{summary}");
        let elo = elo.unwrap();
        assert!(elo.low < 0.0 && elo.high > 0.0);
        assert!(summary.starts_with("An identical engine was rejected"), "{summary}");
    }

    #[test]
    fn an_accept_fails() {
        let lopsided = Pentanomial([100, 800, 2000, 1400, 500]);
        let (passed, _, summary) = judge(&lopsided, BOUNDS, None);
        assert!(!passed);
        assert!(summary.starts_with("An identical engine was accepted"), "{summary}");
        assert!(summary.contains("do not trust results until this is investigated"));
    }

    #[test]
    fn a_rejection_away_from_zero_fails() {
        // Clearly losing: rejected at [0, 3], but the interval does not hold 0.
        let losing = Pentanomial([600, 1400, 2000, 900, 100]);
        let (passed, elo, summary) = judge(&losing, BOUNDS, None);
        assert!(!passed, "{summary}");
        assert!(elo.unwrap().high < 0.0);
        assert!(summary.contains("an interval without 0"));
    }

    #[test]
    fn symmetry_compares_mirrored_counts() {
        let even = symmetry(&Pentanomial([150, 500, 700, 500, 150]));
        assert!(even.passed && even.z_doubles == 0.0 && even.z_halves == 0.0, "{even:?}");
        let tilted = symmetry(&Pentanomial([100, 400, 700, 600, 200]));
        assert!(!tilted.passed && tilted.z_doubles > 3.0, "{tilted:?}");
        assert!(tilted.detail.starts_with("Lopsided over 2000 pairs"));
        assert!(symmetry(&Pentanomial::default()).passed);
        // A lopsided symmetry run fails an otherwise good check.
        let (passed, _, summary) = judge(&Pentanomial([4000, 8000, 16000, 8000, 4000]), BOUNDS, Some(&tilted));
        assert!(!passed && summary.contains("Lopsided"));
    }

    #[test]
    fn the_status_is_current_only_on_the_same_machine_and_tools() {
        let none = status(None, &env("i7", "fastchess-1.8.2"), None);
        assert!(!none.checked && !none.trusted && none.summary.contains("unchecked"));

        let here = status(Some(check(true)), &env("i7", "fastchess-1.8.2"), None);
        assert!(here.checked && here.current && here.trusted && here.passed == Some(true));
        assert!(here.summary.starts_with("Harness checked 2026-10-26"), "{}", here.summary);

        let moved = status(Some(check(true)), &env("ryzen", "fastchess-1.8.2"), None);
        assert!(moved.checked && !moved.current && !moved.trusted && moved.summary.contains("another machine"));

        let tools = status(Some(check(true)), &env("i7", "fastchess-1.9.0"), Some(IdeaId(9)));
        assert!(!tools.current && tools.summary.contains("another fastchess") && tools.running == Some(IdeaId(9)));

        let failed = status(Some(check(false)), &env("i7", "fastchess-1.8.2"), None);
        assert!(failed.current && !failed.trusted && failed.summary.starts_with("The harness check failed"));
    }
}
