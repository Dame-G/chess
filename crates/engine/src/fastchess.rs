//! Driving fastchess v1.8.2, which plays a trial's games: one **round** at a time, a fixed number of game pairs with
//! the cores granted when it starts. The scheduler grows, shrinks or pauses a trial between rounds (see design.md,
//! "Scheduler"), so each round is a self-contained run: its own PGN, its own resume file, its own seed, cheap to start
//! and killed with its engines when cancelled.
//!
//! A round can also be **wound down** before its pairs are played (a live game wants the cores, work of higher
//! priority waits for them, or the SPRT decided mid-round): fastchess is stopped at once and the round keeps the pairs
//! whose two games both finished, read back from its PGN, where each game carries its pair's number (`[Round "n"]`).
//! The games of a pair cut short are dropped from the PGN too, so what the round reports and what it stores agree.
//!
//! Every setting copies Reckless's OpenBench: the opening book played in random order, each opening twice with colours
//! swapped, a 250 ms time margin, and its adjudication, with one deviation: resigning at 3 moves of 300 cp rather than
//! 400 (see [`Adjudication::reckless`]). Counts are reported from the first engine's side (`dev`).

use std::{
    collections::BTreeMap,
    path::{Path, PathBuf},
};

use anyhow::{Result, bail};
use serde::{Deserialize, Serialize};
use tokio_util::sync::CancellationToken;

use crate::{Cancelled, proc};

/// One of the two engines.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Player {
    /// Its name in the PGN and in fastchess's output (`dev`, `base`).
    pub name: String,
    pub path: PathBuf,
    /// UCI options for this engine only, such as `rfp_quad=1140` (SPSA's two sides of a perturbation).
    pub options: Vec<(String, String)>,
}

/// How long each move may take.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Limit {
    /// Seconds per game plus an increment per move, already scaled to this machine by the caller.
    Clock { seconds: f64, increment: f64 },
    /// A fixed number of nodes per move.
    Nodes { nodes: u64 },
}

/// Settings both engines share.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Each {
    pub limit: Limit,
    /// Hash table size in MiB.
    pub hash: u32,
    pub threads: u32,
    /// More UCI options for both engines, after `Hash` and `Threads`.
    pub options: Vec<(String, String)>,
}

impl Each {
    /// The tests' settings for Reckless: `Minimal` on (it reports nothing while it thinks, which is faster to pipe) and
    /// `MoveOverhead` 0 (the time margin covers the pipe's latency instead).
    pub fn reckless(limit: Limit, hash: u32, threads: u32) -> Self {
        Self {
            limit,
            hash,
            threads,
            options: vec![("Minimal".into(), "true".into()), ("MoveOverhead".into(), "0".into())],
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum BookFormat {
    Epd,
    Pgn,
}

/// The opening book, played in random order (seeded by [`Round::seed`]).
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Book {
    pub path: PathBuf,
    pub format: BookFormat,
}

/// Resign adjudication: a game ends when both sides agree, `movecount` moves running, that one side is at least `score`
/// centipawns behind (with `twosided` false, only the losing side's score counts).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Resign {
    pub movecount: u32,
    pub score: u32,
    pub twosided: bool,
}

/// Draw adjudication: from move `movenumber`, a game whose scores stay within `score` centipawns of 0 for `movecount`
/// moves each is drawn.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Draw {
    pub movenumber: u32,
    pub movecount: u32,
    pub score: u32,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Adjudication {
    pub resign: Option<Resign>,
    pub draw: Option<Draw>,
}

impl Adjudication {
    /// The trials' adjudication: three moves at -3 or worse resigns; six moves each within 0.10 from move 30 draws.
    ///
    /// Reckless's OpenBench resigns at -4. Resigning at -3 is a deliberate deviation: replaying 17,944 trial games, it
    /// cut 6 % of the plies played and changed 0.25 % of the results. Draw adjudication is upstream's.
    pub fn reckless() -> Self {
        Self {
            resign: Some(Resign { movecount: 3, score: 300, twosided: false }),
            draw: Some(Draw { movenumber: 30, movecount: 6, score: 10 }),
        }
    }

    pub fn none() -> Self {
        Self { resign: None, draw: None }
    }
}

/// fastchess's own SPRT, which ends a run when it decides. Trials check their SPRT between rounds instead, so a round
/// normally has none; it is here for one-off runs.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub struct Sprt {
    pub elo0: f64,
    pub elo1: f64,
    pub alpha: f64,
    pub beta: f64,
}

/// One run of fastchess.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Round {
    pub fastchess: PathBuf,
    /// The first engine: every count is from its side.
    pub dev: Player,
    pub base: Player,
    pub each: Each,
    /// Games played at once.
    pub concurrency: u32,
    /// Game pairs to play (each opening twice, colours swapped).
    pub pairs: u32,
    pub book: Option<Book>,
    pub adjudication: Adjudication,
    /// Where the games go, with nodes, seldepth, nps, the PV and the clock per move. The file is replaced. fastchess's
    /// resume file goes next to it (`<name>.config.json`), so rounds at once never write over each other's.
    pub pgn: PathBuf,
    /// Seeds the book's order. Give each round its own, or rounds replay the same openings.
    pub seed: Option<u64>,
    /// Restart an engine that crashes or stalls rather than ending the run (the trials' setting).
    pub recover: bool,
    /// Fail on anything fastchess finds suspicious (engine warnings, illegal moves): for smoke runs.
    pub strict: bool,
    pub sprt: Option<Sprt>,
    /// Report the running counts every this many games.
    pub rating_interval: u32,
}

impl Round {
    /// The resume file fastchess keeps beside the PGN.
    pub fn config_path(&self) -> PathBuf {
        self.pgn.with_extension("config.json")
    }
}

/// fastchess's arguments for a round.
pub fn args(r: &Round) -> Vec<String> {
    let path = |p: &Path| p.display().to_string();
    let options = |o: &[(String, String)]| o.iter().map(|(k, v)| format!("option.{k}={v}")).collect::<Vec<_>>();

    let mut each = vec![
        "proto=uci".to_owned(),
        format!("option.Hash={}", r.each.hash),
        format!("option.Threads={}", r.each.threads),
    ];
    each.extend(options(&r.each.options));
    match r.each.limit {
        Limit::Clock { seconds, increment } => {
            each.push(format!("tc={}+{}", clock_number(seconds), clock_number(increment)));
            // A 250 ms margin, as OpenBench gives: with MoveOverhead=0, pipe latency alone would lose games on time.
            each.push("timemargin=250".to_owned());
        }
        Limit::Nodes { nodes } => each.push(format!("nodes={nodes}")),
    }

    let mut args = Vec::new();
    for p in [&r.dev, &r.base] {
        args.extend(["-engine".to_owned(), format!("cmd={}", path(&p.path)), format!("name={}", p.name)]);
        args.extend(options(&p.options));
    }
    args.push("-each".into());
    args.extend(each);
    if let Some(book) = &r.book {
        let format = match book.format {
            BookFormat::Epd => "epd",
            BookFormat::Pgn => "pgn",
        };
        args.extend([
            "-openings".into(),
            format!("file={}", path(&book.path)),
            format!("format={format}"),
            "order=random".into(),
        ]);
    }
    args.extend([
        "-repeat".into(),
        "-games".into(),
        "2".into(),
        "-rounds".into(),
        r.pairs.max(1).to_string(),
        "-concurrency".into(),
        r.concurrency.max(1).to_string(),
        "-ratinginterval".into(),
        r.rating_interval.max(1).to_string(),
        "-config".into(),
        format!("outname={}", path(&r.config_path())),
        "-pgnout".into(),
        format!("file={}", path(&r.pgn)),
        "nodes=true".into(),
        "seldepth=true".into(),
        "nps=true".into(),
        "pv=true".into(),
        "timeleft=true".into(),
    ]);
    if let Some(seed) = r.seed {
        args.extend(["-srand".into(), seed.to_string()]);
    }
    if let Some(resign) = r.adjudication.resign {
        args.extend(["-resign".into(), format!("movecount={}", resign.movecount), format!("score={}", resign.score)]);
        if resign.twosided {
            args.push("twosided=true".into());
        }
    }
    if let Some(draw) = r.adjudication.draw {
        args.extend([
            "-draw".into(),
            format!("movenumber={}", draw.movenumber),
            format!("movecount={}", draw.movecount),
            format!("score={}", draw.score),
        ]);
    }
    if let Some(sprt) = r.sprt {
        args.extend([
            "-sprt".into(),
            format!("elo0={}", sprt.elo0),
            format!("elo1={}", sprt.elo1),
            format!("alpha={}", sprt.alpha),
            format!("beta={}", sprt.beta),
            "model=normalized".into(),
        ]);
    }
    if r.recover {
        args.push("-recover".into());
    }
    if r.strict {
        args.push("-strict".into());
    }
    args
}

/// Seconds as OpenBench passes them: at most two decimals, no trailing zeros (`8`, `0.08`, `24.91`).
fn clock_number(seconds: f64) -> String {
    let s = format!("{:.2}", (seconds * 100.0).round() / 100.0);
    s.trim_end_matches('0').trim_end_matches('.').to_owned()
}

/// The counts of a run so far, from `dev`'s side.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Totals {
    /// Game pairs scoring 0, ½, 1, 1½ and 2 points for dev: LL, LD, DD or WL, WD, WW.
    pub penta: [u32; 5],
    pub wins: u32,
    pub draws: u32,
    pub losses: u32,
}

impl Totals {
    pub fn games(&self) -> u32 {
        self.wins + self.draws + self.losses
    }
}

/// One finished game, from fastchess's `Finished game` line.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct FinishedGame {
    /// From 1, in the order games were started.
    pub number: u32,
    pub white: String,
    pub black: String,
    /// `1-0`, `0-1`, `1/2-1/2` or `*`.
    pub result: String,
    /// fastchess's reason in words (`White mates`, `Draw by adjudication`…).
    pub reason: String,
}

/// What a running round reports.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum Progress {
    Finished(FinishedGame),
    /// Running counts, at each rating interval and at the end.
    Totals(Totals),
}

/// What a round played.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct RoundResult {
    pub totals: Totals,
    pub pgn: PathBuf,
    /// It was wound down before its pairs were all played: `totals` and the PGN hold the pairs that finished.
    #[serde(default)]
    pub ended_early: bool,
}

/// Plays a round to the end and returns its counts and its PGN. Each progress report goes to `on_progress`; every
/// other line of fastchess's output (the per-game lines are in the PGN) goes to `log`. A cancel kills fastchess and its
/// engines and returns [`Cancelled`]. A wind-down kills them too, but returns the pairs that had finished (see
/// [`complete_pairs`]), with `ended_early` set.
pub async fn run_round(
    round: &Round,
    cancel: &CancellationToken,
    wind_down: &CancellationToken,
    on_progress: &mut (dyn FnMut(Progress) + Send),
    log: &mut (dyn FnMut(&str) + Send),
) -> Result<RoundResult> {
    for p in [&round.pgn, &round.config_path()] {
        let _ = std::fs::remove_file(p);
    }
    if let Some(dir) = round.pgn.parent().filter(|d| !d.as_os_str().is_empty()) {
        std::fs::create_dir_all(dir)?;
    }
    let args = args(round);
    let command = proc::Command::new(&round.fastchess).args(&args);
    log(&command.display());
    let mut child = proc::spawn(&command)?;
    let mut lines = proc::lines(&mut child);
    let mut parser = Parser::default();
    let mut tail: Vec<String> = Vec::new();
    loop {
        tokio::select! {
            line = lines.recv() => match line {
                Some(line) => {
                    if let Some(p) = parser.line(&line) {
                        on_progress(p);
                    }
                    if !line.starts_with("Started game") && !line.starts_with("Finished game") {
                        log(&line);
                        tail.push(line);
                        if tail.len() > 20 {
                            tail.remove(0);
                        }
                    }
                }
                None => break,
            },
            _ = cancel.cancelled() => {
                child.kill_tree().await?;
                return Err(Cancelled.into());
            }
            _ = wind_down.cancelled() => {
                child.kill_tree().await?;
                let text = std::fs::read_to_string(&round.pgn).unwrap_or_default();
                let (totals, kept) = complete_pairs(&text, &round.dev.name);
                std::fs::write(&round.pgn, kept)?;
                log(&format!("Wound down early: kept {} finished pairs", totals.penta.iter().sum::<u32>()));
                return Ok(RoundResult { totals, pgn: round.pgn.clone(), ended_early: true });
            }
        }
    }
    let status = child.wait().await?;
    if !status.success() {
        bail!("fastchess exited with {status}: {}", tail.join(" | "));
    }
    Ok(RoundResult { totals: parser.totals, pgn: round.pgn.clone(), ended_early: false })
}

/// The pairs of a round's PGN whose two games both finished, and the PGN of just those games. fastchess numbers each
/// game with its pair (`[Round "n"]`, one opening played twice) and writes a game whole once it ends, so a game whose
/// movetext does not end in its result was cut off as fastchess was stopped, and does not count. Counts are from
/// `dev`'s side (a game's `White` name says which side dev had).
pub fn complete_pairs(pgn: &str, dev: &str) -> (Totals, String) {
    let mut pairs: BTreeMap<String, Vec<(&str, u32)>> = BTreeMap::new();
    for text in split_games(pgn) {
        let header = |name: &str| {
            let prefix = format!("[{name} \"");
            text.lines().find_map(|l| l.trim().strip_prefix(prefix.as_str())?.strip_suffix("\"]").map(str::to_owned))
        };
        let (Some(round), Some(white), Some(result)) = (header("Round"), header("White"), header("Result")) else {
            continue;
        };
        // Half points for White: 2 a win, 1 a draw, 0 a loss.
        let white_points = match result.as_str() {
            "1-0" => 2,
            "1/2-1/2" => 1,
            "0-1" => 0,
            _ => continue,
        };
        if !text.trim_end().ends_with(result.as_str()) {
            continue;
        }
        let dev_points = if white == dev { white_points } else { 2 - white_points };
        pairs.entry(round).or_default().push((text, dev_points));
    }
    let mut totals = Totals::default();
    let mut kept = String::new();
    for games in pairs.values().filter(|g| g.len() == 2) {
        let points: u32 = games.iter().map(|(_, p)| p).sum();
        totals.penta[points as usize] += 1;
        for (text, p) in games {
            match p {
                2 => totals.wins += 1,
                1 => totals.draws += 1,
                _ => totals.losses += 1,
            }
            kept.push_str(text.trim_end());
            kept.push_str("\n\n");
        }
    }
    (totals, kept)
}

/// A PGN's games as text, each from its first header line (`[Event`) to the next game's.
fn split_games(pgn: &str) -> Vec<&str> {
    let mut starts: Vec<usize> = Vec::new();
    let mut at = 0;
    for line in pgn.split_inclusive('\n') {
        if line.trim_start().starts_with("[Event ") {
            starts.push(at);
        }
        at += line.len();
    }
    starts.iter().enumerate().map(|(i, s)| &pgn[*s..starts.get(i + 1).copied().unwrap_or(pgn.len())]).collect()
}

/// Reads fastchess's output a line at a time, keeping the running totals.
#[derive(Default)]
pub struct Parser {
    pub totals: Totals,
}

impl Parser {
    pub fn line(&mut self, line: &str) -> Option<Progress> {
        let line = line.trim();
        if let Some(game) = parse_finished(line) {
            return Some(Progress::Finished(game));
        }
        if let Some((wins, losses, draws)) = parse_games(line) {
            self.totals.wins = wins;
            self.totals.losses = losses;
            self.totals.draws = draws;
            return None;
        }
        if let Some(penta) = parse_ptnml(line) {
            self.totals.penta = penta;
            return Some(Progress::Totals(self.totals));
        }
        None
    }
}

/// `Ptnml(0-2): [LL, LD, WL+DD, WD, WW], WL/DD Ratio: ...`
fn parse_ptnml(line: &str) -> Option<[u32; 5]> {
    let inner = line.strip_prefix("Ptnml(0-2): [")?.split(']').next()?;
    let mut counts = [0u32; 5];
    let mut parts = inner.split(',');
    for c in &mut counts {
        *c = parts.next()?.trim().parse().ok()?;
    }
    Some(counts)
}

/// `Games: 20, Wins: 5, Losses: 4, Draws: 11, Points: 10.5 (52.50 %)`: wins, losses and draws of the first engine.
fn parse_games(line: &str) -> Option<(u32, u32, u32)> {
    let rest = line.strip_prefix("Games: ")?;
    let field = |name: &str| -> Option<u32> {
        let at = rest.find(&format!("{name}: "))? + name.len() + 2;
        rest[at..].split(|c: char| !c.is_ascii_digit()).next()?.parse().ok()
    };
    Some((field("Wins")?, field("Losses")?, field("Draws")?))
}

/// `Finished game 3 (dev vs base): 1-0 {White mates}`
fn parse_finished(line: &str) -> Option<FinishedGame> {
    let rest = line.strip_prefix("Finished game ")?;
    let (number, rest) = rest.split_once(" (")?;
    let (players, rest) = rest.split_once("): ")?;
    let (white, black) = players.split_once(" vs ")?;
    let (result, reason) = match rest.split_once(" {") {
        Some((result, reason)) => (result, reason.trim_end_matches('}')),
        None => (rest, ""),
    };
    Some(FinishedGame {
        number: number.trim().parse().ok()?,
        white: white.to_owned(),
        black: black.to_owned(),
        result: result.trim().to_owned(),
        reason: reason.to_owned(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn round(limit: Limit) -> Round {
        Round {
            fastchess: "fc".into(),
            dev: Player { name: "dev".into(), path: "dev".into(), options: vec![("rfp_quad".into(), "1200".into())] },
            base: Player { name: "base".into(), path: "base".into(), options: vec![] },
            each: Each::reckless(limit, 16, 1),
            concurrency: 14,
            pairs: 56,
            book: Some(Book { path: "book.epd".into(), format: BookFormat::Epd }),
            adjudication: Adjudication::reckless(),
            pgn: Path::new("trials").join("7").join("round-3.pgn"),
            seed: Some(42),
            recover: true,
            strict: false,
            sprt: None,
            rating_interval: 10,
        }
    }

    #[test]
    fn builds_reckless_style_arguments() {
        let joined = args(&round(Limit::Clock { seconds: 8.0, increment: 0.08 })).join(" ");
        let config = format!("-config outname={}", Path::new("trials").join("7").join("round-3.config.json").display());
        assert!(joined.contains(&config), "{joined}");
        assert!(
            joined.starts_with("-engine cmd=dev name=dev option.rfp_quad=1200 -engine cmd=base name=base -each"),
            "{joined}"
        );
        assert!(joined.contains("proto=uci option.Hash=16 option.Threads=1 option.Minimal=true option.MoveOverhead=0 tc=8+0.08 timemargin=250"), "{joined}");
        assert!(
            joined.contains(
                "-openings file=book.epd format=epd order=random -repeat -games 2 -rounds 56 -concurrency 14"
            )
        );
        assert!(joined.contains("-pgnout file="));
        assert!(joined.contains("nodes=true seldepth=true nps=true pv=true timeleft=true"));
        assert!(joined.contains("-srand 42"));
        assert!(joined.contains("-resign movecount=3 score=300 -draw movenumber=30 movecount=6 score=10"), "{joined}");
        assert!(joined.ends_with("-recover"));
        assert!(!joined.contains("-sprt") && !joined.contains("-strict"));
    }

    #[test]
    fn plays_nodes_and_smoke_settings() {
        let mut r = round(Limit::Nodes { nodes: 25_000 });
        r.strict = true;
        r.recover = false;
        r.book = None;
        r.adjudication = Adjudication::none();
        r.sprt = Some(Sprt { elo0: 0.0, elo1: 3.0, alpha: 0.05, beta: 0.1 });
        let joined = args(&r).join(" ");
        assert!(joined.contains("nodes=25000") && !joined.contains("tc="), "{joined}");
        assert!(!joined.contains("-openings") && !joined.contains("-resign") && !joined.contains("-draw"));
        assert!(joined.contains("-sprt elo0=0 elo1=3 alpha=0.05 beta=0.1 model=normalized"));
        assert!(joined.ends_with("-strict"));
    }

    #[test]
    fn writes_clocks_as_openbench_does() {
        assert_eq!(clock_number(8.0), "8");
        assert_eq!(clock_number(0.08), "0.08");
        assert_eq!(clock_number(24.905), "24.91");
        assert_eq!(clock_number(11.0), "11");
        assert_eq!(clock_number(0.1), "0.1");
    }

    #[test]
    fn reads_fastchess_output() {
        let mut p = Parser::default();
        let out = "Started game 1 of 20 (dev vs base)\n\
            Finished game 1 (dev vs base): 1-0 {White mates}\n\
            Finished game 2 (base vs dev): 1/2-1/2 {Draw by adjudication}\n\
            --------------------------------------------------\n\
            Results of dev vs base (8+0.08, 1t, 16MB, UHO_Lichess_4852_v1.epd):\n\
            Elo: 12.34 +/- 20.00, nElo: 15.00 +/- 25.00\n\
            LOS: 80.00 %, DrawRatio: 40.00 %, PairsRatio: 1.20\n\
            Games: 20, Wins: 7, Losses: 5, Draws: 8, Points: 11.0 (55.00 %)\n\
            Ptnml(0-2): [1, 2, 3, 2, 2], WL/DD Ratio: 0.50\n\
            --------------------------------------------------";
        let reports: Vec<Progress> = out.lines().filter_map(|l| p.line(l)).collect();
        assert_eq!(reports.len(), 3);
        assert_eq!(
            reports[0],
            Progress::Finished(FinishedGame {
                number: 1,
                white: "dev".into(),
                black: "base".into(),
                result: "1-0".into(),
                reason: "White mates".into()
            })
        );
        let Progress::Finished(g) = &reports[1] else { panic!() };
        assert_eq!((g.white.as_str(), g.result.as_str()), ("base", "1/2-1/2"));
        let totals = Totals { penta: [1, 2, 3, 2, 2], wins: 7, draws: 8, losses: 5 };
        assert_eq!(reports[2], Progress::Totals(totals));
        assert_eq!(totals.games(), 20);
    }

    fn game(round: u32, white: &str, black: &str, result: &str, whole: bool) -> String {
        let moves =
            if whole { format!("1. e4 {{+0.20/10 0.100s}} e5 2. Nf3 {result}") } else { "1. e4 e5 2. N".into() };
        format!(
            "[Event \"Fastchess Tournament\"]\n[Round \"{round}\"]\n[White \"{white}\"]\n[Black \"{black}\"]\n\
             [Result \"{result}\"]\n\n{moves}\n\n"
        )
    }

    #[test]
    fn a_wound_down_round_keeps_only_its_finished_pairs() {
        let pgn = [
            game(2, "base", "dev", "0-1", true),
            game(1, "dev", "base", "1-0", true),
            game(3, "dev", "base", "1/2-1/2", true),
            game(1, "base", "dev", "1-0", true),
            game(2, "dev", "base", "1-0", true),
            // Pair 3's second game was cut off mid-write; pair 4 only started.
            game(3, "base", "dev", "1/2-1/2", false),
            game(4, "dev", "base", "0-1", true),
        ]
        .concat();
        let (totals, kept) = complete_pairs(&pgn, "dev");
        // Pair 1: dev won as White, lost as Black (1 point); pair 2: dev won both (2 points).
        assert_eq!(totals, Totals { penta: [0, 0, 1, 0, 1], wins: 3, draws: 0, losses: 1 });
        assert_eq!(kept.matches("[Event ").count(), 4);
        assert!(!kept.contains("[Round \"3\"]") && !kept.contains("[Round \"4\"]"), "{kept}");
        assert_eq!(complete_pairs("", "dev"), (Totals::default(), String::new()));
    }

    #[test]
    fn ignores_lines_that_are_not_counts() {
        assert_eq!(parse_ptnml("LLR: 0.02 (0.6%) (-2.25, 2.89) [0.00, 3.00]"), None);
        assert_eq!(parse_games("Games played: 3"), None);
        assert_eq!(parse_finished("Finished match"), None);
    }

    /// A real round: two copies of a Reckless binary at a tiny node count, through fastchess.
    /// `FASTCHESS=… RECKLESS_BIN=… cargo test -p dameless-engine -- --ignored plays_a_real_round`.
    #[tokio::test]
    #[ignore = "needs FASTCHESS and RECKLESS_BIN"]
    async fn plays_a_real_round() {
        let fastchess = std::env::var("FASTCHESS").expect("FASTCHESS");
        let bin = PathBuf::from(std::env::var("RECKLESS_BIN").expect("RECKLESS_BIN"));
        let dir = crate::testing::TempDir::new("fastchess");
        let mut r = round(Limit::Nodes { nodes: 5_000 });
        r.fastchess = fastchess.into();
        r.dev = Player { name: "dev".into(), path: bin.clone(), options: vec![] };
        r.base = Player { name: "base".into(), path: bin, options: vec![] };
        r.book = std::env::var("BOOK").ok().map(|b| Book { path: b.into(), format: BookFormat::Epd });
        r.pairs = 4;
        r.concurrency = 2;
        r.pgn = dir.path().join("round.pgn");
        let mut finished = 0;
        let result = run_round(
            &r,
            &CancellationToken::new(),
            &CancellationToken::new(),
            &mut |p| {
                if matches!(p, Progress::Finished(_)) {
                    finished += 1;
                }
            },
            &mut |l| println!("{l}"),
        )
        .await
        .unwrap();
        assert_eq!(result.totals.games(), 8);
        assert_eq!(result.totals.penta.iter().sum::<u32>(), 4);
        assert_eq!(finished, 8);
        assert!(std::fs::read_to_string(&result.pgn).unwrap().contains("[White \""));
    }
}
