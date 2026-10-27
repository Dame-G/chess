//! Checking the harness: does this machine's testing treat two identical engines as identical?
//!
//! The check plays the champion against a branch identical to it (only a comment differs, so the bench is the same),
//! in two parts:
//!
//! - **Determinism.** A fixed number of pairs at a fixed node count, one thread and a fresh hash each game (fastchess
//!   sends `ucinewgame` before every game, and Reckless clears its hash, histories and correction tables on it). At a
//!   fixed node count on one thread Reckless is deterministic, so both games of a pair (one opening, colours swapped)
//!   must be the same game move for move, and every pair must score exactly 1. Any pair that does not, or any two
//!   games from one opening that differ, fails the check and names the opening: something is nondeterministic (the
//!   hash carried over, a time limit, a build that is not what it claims) or the engines were mixed up.
//! - **Symmetry.** A fixed number of pairs at VSTC, a timed run where the games do differ. It passes when the
//!   identical engine's Elo interval contains 0 and the pentanomial is symmetric: pairs lost twice about as many as
//!   pairs won twice (LL ≈ WW), lost-and-drawn about as many as won-and-drawn (LD ≈ WD), within 3 standard errors.
//!   This catches colour or order bias, dev and base mixed up, or a broken book.
//!
//! The check passes only when both parts pass. It replaced an SPRT at VSTC [0, 3], which between identical engines
//! averages some 53,000 games: these two parts take a few hours and say more.
//!
//! The result belongs to the machine and the tools it was played with: a check on another CPU, or with another
//! fastchess or book, says nothing about this one, so the status calls it out of date.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};
use utoipa::ToSchema;

use crate::{
    ideas::Preset,
    ids::{IdeaId, Timestamp},
    stats::{self, Pentanomial},
    version::Estimate,
};

/// The tag every harness check idea carries.
pub const TAG: &str = "harness";
/// The branch the app makes for the check: `main`'s tip plus a comment.
pub const BRANCH: &str = "harness/identical";
/// The determinism part's preset: 25,000 nodes a move, one thread, 16 MiB of hash.
pub const DETERMINISM_PRESET: Preset = Preset::Nodes;
/// The determinism part's usual length, in game pairs.
pub const DETERMINISM_PAIRS: u32 = 1_000;
/// The symmetry part's preset: quick timed games.
pub const SYMMETRY_PRESET: Preset = Preset::Vstc;
/// The symmetry part's usual length, in game pairs.
pub const SYMMETRY_PAIRS: u32 = 4_000;
/// How far apart, in standard errors, mirrored pentanomial counts may be before the pairs are called lopsided. Loose
/// on purpose: this catches gross bias, and a false alarm costs a needless investigation.
pub const SYMMETRY_Z: f64 = 3.0;
/// At most this many failed openings are kept with a check.
pub const OPENINGS_KEPT: usize = 20;

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

/// One game of the determinism part, as the check compares them.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PlayedGame {
    /// The start position (FEN), or `startpos`.
    pub opening: String,
    /// The moves, in UCI.
    pub moves: Vec<String>,
    /// `1-0`, `0-1`, `1/2-1/2`, or `*` when it did not finish.
    pub result: String,
}

/// How the determinism part came out.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, ToSchema)]
pub struct Determinism {
    pub passed: bool,
    /// Pairs played.
    pub pairs: u64,
    /// Nodes a move.
    pub nodes: u64,
    /// Pairs that did not score exactly 1, or whose games differed.
    pub uneven: u64,
    /// The openings (start FENs) whose games differed, at most [`OPENINGS_KEPT`].
    pub openings: Vec<String>,
    /// One sentence for people.
    pub detail: String,
}

/// The determinism verdict of `pairs_wanted` pairs at `nodes`: every pair must score exactly 1 (`penta` is all in
/// its middle), and every game from one opening must be the same game.
pub fn determinism(penta: &Pentanomial, games: &[PlayedGame], pairs_wanted: u64, nodes: u64) -> Determinism {
    let [ll, ld, _, wd, ww] = penta.0;
    let lopsided = ll + ld + wd + ww;
    let mut by_opening: BTreeMap<&str, Vec<&PlayedGame>> = BTreeMap::new();
    for g in games {
        by_opening.entry(g.opening.as_str()).or_default().push(g);
    }
    let differing: Vec<(&str, u64)> = by_opening
        .iter()
        .filter(|(_, gs)| gs.iter().any(|g| g.moves != gs[0].moves || g.result != gs[0].result || g.result == "*"))
        .map(|(opening, gs)| (*opening, (gs.len() as u64 / 2).max(1)))
        .collect();
    let uneven = lopsided.max(differing.iter().map(|(_, n)| n).sum());
    let pairs = penta.pairs();
    let openings: Vec<String> = differing.iter().take(OPENINGS_KEPT).map(|(o, _)| (*o).to_owned()).collect();
    let at = format!("at {} nodes", thousands(nodes));
    let (passed, detail) = if pairs < pairs_wanted {
        (false, format!("Only {pairs} of {pairs_wanted} pairs {at} were played"))
    } else if uneven == 0 {
        (true, format!("Deterministic: all {pairs} pairs {at} were the same game twice"))
    } else {
        let named = if openings.is_empty() {
            String::new()
        } else {
            format!(" (openings: {}{})", openings.join("; "), if differing.len() > openings.len() { "; …" } else { "" })
        };
        (
            false,
            format!(
                "Not deterministic: {uneven} of {pairs} pairs {at} were not the same game twice{named} \
                 (LL {ll}, LD {ld}, WD {wd}, WW {ww})"
            ),
        )
    };
    Determinism { passed, pairs, nodes, uneven, openings, detail }
}

/// How symmetric the timed pairs of identical engines came out.
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

/// The symmetry verdict of a run between identical engines: the mirrored counts only.
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

/// The check's verdict from its two parts: passed when the determinism part passed, the symmetry run played its pairs,
/// its Elo interval contains 0, and its pentanomial is symmetric. Returns the outcome, the identical engine's Elo, the
/// symmetry verdict and one sentence. With no symmetry run (the determinism part failed first), only the first part
/// speaks.
pub fn judge(
    determinism: &Determinism,
    symmetry_run: Option<(&Pentanomial, u64)>,
) -> (bool, Option<Estimate>, Option<Symmetry>, String) {
    const INVESTIGATE: &str = "do not trust results until this is investigated";
    if !determinism.passed {
        return (false, None, None, format!("{}: {INVESTIGATE}", determinism.detail));
    }
    let Some((penta, wanted)) = symmetry_run else {
        return (false, None, None, format!("{}, but the symmetry run did not finish", determinism.detail));
    };
    let elo = stats::estimate(penta).map(|e| e.elo_interval());
    let sym = symmetry(penta);
    let games = penta.pairs() * 2;
    let elo_text = elo.map(|e| format!("{} Elo", e.text())).unwrap_or_else(|| "no estimate".into());
    let contains_zero = elo.is_some_and(|e| e.low <= 0.0 && e.high >= 0.0);
    let first = format!("Deterministic over {} pairs", determinism.pairs);
    let (passed, summary) = if penta.pairs() < wanted {
        (false, format!("{first}, but the symmetry run played only {} of {wanted} pairs", penta.pairs()))
    } else if !contains_zero {
        (
            false,
            format!(
                "{first}, but an identical engine measured {elo_text} over {games} games, an interval without 0: \
                 {INVESTIGATE}"
            ),
        )
    } else if !sym.passed {
        (false, format!("{first}, but {}: {INVESTIGATE}", lower_first(&sym.detail)))
    } else {
        (true, format!("{first}; {} at {elo_text}", lower_first(&sym.detail)))
    };
    (passed, elo, Some(sym), summary)
}

fn lower_first(s: &str) -> String {
    let mut c = s.chars();
    c.next().map(|f| f.to_lowercase().chain(c).collect()).unwrap_or_default()
}

/// `25,000`.
fn thousands(n: u64) -> String {
    let digits = n.to_string();
    let mut out = String::new();
    for (i, ch) in digits.chars().enumerate() {
        if i > 0 && (digits.len() - i).is_multiple_of(3) {
            out.push(',');
        }
        out.push(ch);
    }
    out
}

/// A finished check, as stored for the machine (`meta` key `harness.check`).
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, ToSchema)]
pub struct HarnessCheck {
    pub machine: Machine,
    pub at: Timestamp,
    /// The idea that ran it.
    pub idea: IdeaId,
    pub passed: bool,
    /// The determinism part; none for a check made before it existed (an SPRT at VSTC [0, 3]).
    #[serde(default)]
    pub determinism: Option<Determinism>,
    /// Logistic Elo of the identical engine ± 95 %, from the symmetry run.
    pub elo: Option<Estimate>,
    /// The symmetry part; none when the determinism part failed first.
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
    /// The last check, in full: its determinism and symmetry parts.
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
            determinism: None,
            elo: None,
            symmetry: None,
            engine_commit: "abc".into(),
            fastchess: Some("fastchess-1.8.2".into()),
            book: Some("UHO.epd".into()),
            summary: "Deterministic over 1000 pairs".into(),
        }
    }

    fn game(opening: &str, moves: &str, result: &str) -> PlayedGame {
        PlayedGame {
            opening: opening.into(),
            moves: moves.split_whitespace().map(str::to_owned).collect(),
            result: result.into(),
        }
    }

    fn mirrored(pairs: u64) -> (Pentanomial, Vec<PlayedGame>) {
        let games = (0..pairs)
            .flat_map(|i| {
                let fen = format!("fen {i}");
                [game(&fen, "e2e4 e7e5", "1-0"), game(&fen, "e2e4 e7e5", "1-0")]
            })
            .collect();
        (Pentanomial([0, 0, pairs, 0, 0]), games)
    }

    #[test]
    fn identical_games_in_every_pair_are_deterministic() {
        let (penta, games) = mirrored(1000);
        let d = determinism(&penta, &games, 1000, 25_000);
        assert!(d.passed && d.uneven == 0, "{d:?}");
        assert_eq!(d.detail, "Deterministic: all 1000 pairs at 25,000 nodes were the same game twice");
    }

    #[test]
    fn a_pair_that_differs_fails_and_names_its_opening() {
        let (_, mut games) = mirrored(10);
        // Same score (two draws), different games: still not deterministic.
        games[4] = game("fen 2", "e2e4 c7c5", "1/2-1/2");
        games[5] = game("fen 2", "e2e4 e7e5", "1/2-1/2");
        let d = determinism(&Pentanomial([0, 0, 10, 0, 0]), &games, 10, 25_000);
        assert!(!d.passed && d.uneven == 1, "{d:?}");
        assert_eq!(d.openings, ["fen 2"]);
        assert!(d.detail.starts_with("Not deterministic: 1 of 10 pairs") && d.detail.contains("fen 2"), "{}", d.detail);
        // A pair that did not score 1 fails even without its games.
        let lopsided = determinism(&Pentanomial([0, 1, 9, 0, 0]), &[], 10, 25_000);
        assert!(!lopsided.passed && lopsided.uneven == 1 && lopsided.detail.contains("LD 1"), "{lopsided:?}");
        // Too few pairs played fails.
        assert!(!determinism(&Pentanomial([0, 0, 5, 0, 0]), &[], 10, 25_000).passed);
    }

    #[test]
    fn both_parts_must_pass() {
        let (penta, games) = mirrored(1000);
        let det = determinism(&penta, &games, 1000, 25_000);
        let even = Pentanomial([400, 800, 1600, 800, 400]);
        let (passed, elo, sym, summary) = judge(&det, Some((&even, 4000)));
        assert!(passed, "{summary}");
        let elo = elo.unwrap();
        assert!(elo.low < 0.0 && elo.high > 0.0);
        assert!(sym.unwrap().passed);
        assert!(summary.starts_with("Deterministic over 1000 pairs; symmetric over 4000 pairs"), "{summary}");

        // Lopsided doubles fail, though the Elo interval may hold 0.
        let tilted = Pentanomial([330, 940, 1600, 660, 470]);
        let (passed, _, sym, summary) = judge(&det, Some((&tilted, 4000)));
        assert!(!passed && !sym.unwrap().passed && summary.contains("lopsided"), "{summary}");

        // An estimate away from 0 fails.
        let losing = Pentanomial([600, 1400, 2000, 900, 100]);
        let (passed, _, _, summary) = judge(&det, Some((&losing, 5000)));
        assert!(!passed && summary.contains("an interval without 0"), "{summary}");

        // A short symmetry run fails.
        let (passed, _, _, summary) = judge(&det, Some((&even, 5000)));
        assert!(!passed && summary.contains("only 4000 of 5000"), "{summary}");

        // A failed determinism part decides alone.
        let bad = determinism(&Pentanomial([1, 0, 999, 0, 0]), &[], 1000, 25_000);
        let (passed, elo, sym, summary) = judge(&bad, None);
        assert!(!passed && elo.is_none() && sym.is_none());
        assert!(summary.ends_with("do not trust results until this is investigated"), "{summary}");
    }

    #[test]
    fn symmetry_compares_mirrored_counts() {
        let even = symmetry(&Pentanomial([150, 500, 700, 500, 150]));
        assert!(even.passed && even.z_doubles == 0.0 && even.z_halves == 0.0, "{even:?}");
        let tilted = symmetry(&Pentanomial([100, 400, 700, 600, 200]));
        assert!(!tilted.passed && tilted.z_doubles > 3.0, "{tilted:?}");
        assert!(tilted.detail.starts_with("Lopsided over 2000 pairs"));
        assert!(symmetry(&Pentanomial::default()).passed);
    }

    #[test]
    fn an_older_check_still_reads() {
        let old = serde_json::json!({
            "machine": { "cpu": "i7", "cores": 8, "os": "Windows 11" },
            "at": "2026-10-26T10:00:00.000Z", "idea": 4, "passed": true, "elo": null, "symmetry": null,
            "engine_commit": "abc", "fastchess": "fastchess-1.8.2", "book": "UHO.epd",
            "summary": "An identical engine was rejected",
        });
        let c: HarnessCheck = serde_json::from_value(old).unwrap();
        assert!(c.passed && c.determinism.is_none());
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
