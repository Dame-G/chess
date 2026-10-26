//! Talking to an engine over UCI: the handshake, options, `position` and `go`, `stop`, and the `info` and `bestmove`
//! lines that come back.
//!
//! **Scores here are as UCI gives them: from the point of view of the side to move in the searched position** (the
//! engine's own). The domain stores every score from White's point of view; the crates above flip them when they map
//! an [`Info`] onto `core`'s types.

use std::{path::Path, time::Duration};

use anyhow::{Context, Result, bail};
use serde::{Deserialize, Serialize};
use tokio::{io::AsyncWriteExt, sync::mpsc};

use crate::proc;

/// How long an engine has to answer `uci`. Reckless answers at once; one that does not is broken or not an engine.
pub const UCIOK_TIMEOUT: Duration = Duration::from_secs(10);

/// How long an engine has to answer `isready`. Longer than for `uciok`, since a large `Hash` is allocated and cleared
/// before it answers.
pub const READYOK_TIMEOUT: Duration = Duration::from_secs(30);

/// A search score from the side to move's point of view, as UCI reports it.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Score {
    /// Centipawns.
    Cp(i32),
    /// Mate in this many moves; negative when the side to move is mated, 0 when it is mated now.
    Mate(i32),
}

/// Whether a score is exact or only a bound (an aspiration window failed high or low).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Bound {
    #[default]
    Exact,
    /// `lowerbound`: the true score is at least this.
    Lower,
    /// `upperbound`: the true score is at most this.
    Upper,
}

/// One `info` line that carries a score.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Info {
    pub depth: u32,
    pub seldepth: Option<u32>,
    /// Which line of a MultiPV search, from 1.
    pub multipv: u32,
    pub score: Score,
    pub bound: Bound,
    pub nodes: u64,
    pub nps: u64,
    /// Milliseconds since the search started.
    pub time_ms: u64,
    /// Per mille of the hash table in use.
    pub hashfull: Option<u32>,
    /// The principal variation in UCI notation (`e2e4`, `e7e8q`). Can be empty when the engine sends none.
    pub pv: Vec<String>,
}

impl Info {
    /// Parses an `info` line. Lines without a score (`info string`, currmove updates) are `None`.
    pub fn parse(line: &str) -> Option<Self> {
        let mut words = line.split_whitespace();
        if words.next()? != "info" {
            return None;
        }
        let mut info = Info {
            depth: 0,
            seldepth: None,
            multipv: 1,
            score: Score::Cp(0),
            bound: Bound::Exact,
            nodes: 0,
            nps: 0,
            time_ms: 0,
            hashfull: None,
            pv: Vec::new(),
        };
        let mut scored = false;
        while let Some(word) = words.next() {
            match word {
                "depth" => info.depth = words.next()?.parse().ok()?,
                "seldepth" => info.seldepth = Some(words.next()?.parse().ok()?),
                "multipv" => info.multipv = words.next()?.parse().ok()?,
                "nodes" => info.nodes = words.next()?.parse().ok()?,
                "nps" => info.nps = words.next()?.parse().ok()?,
                "time" => info.time_ms = words.next()?.parse().ok()?,
                "hashfull" => info.hashfull = Some(words.next()?.parse().ok()?),
                "score" => {
                    info.score = match words.next()? {
                        "cp" => Score::Cp(words.next()?.parse().ok()?),
                        "mate" => Score::Mate(words.next()?.parse().ok()?),
                        _ => return None,
                    };
                    scored = true;
                }
                "lowerbound" => info.bound = Bound::Lower,
                "upperbound" => info.bound = Bound::Upper,
                "pv" => info.pv = words.by_ref().map(str::to_owned).collect(),
                "string" => return None,
                _ => {}
            }
        }
        scored.then_some(info)
    }
}

/// The `bestmove` line that ends a search.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct BestMove {
    /// The move, or `None` when there is none (`(none)` or `0000`: the side to move is mated or stalemated).
    pub best: Option<String>,
    pub ponder: Option<String>,
}

impl BestMove {
    pub fn parse(line: &str) -> Option<Self> {
        let mut words = line.split_whitespace();
        if words.next()? != "bestmove" {
            return None;
        }
        let real = |m: &str| (m != "(none)" && m != "0000").then(|| m.to_owned());
        let best = words.next().and_then(real);
        let ponder = match (words.next(), words.next()) {
            (Some("ponder"), Some(m)) => real(m),
            _ => None,
        };
        Some(Self { best, ponder })
    }
}

/// What a running search sends.
#[derive(Clone, Debug, PartialEq)]
pub enum SearchEvent {
    Info(Info),
    BestMove(BestMove),
}

/// A position to search: the start position or a FEN, then moves in UCI notation.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Position {
    /// `None` is the standard start position.
    pub fen: Option<String>,
    pub moves: Vec<String>,
}

impl Position {
    pub fn startpos() -> Self {
        Self::default()
    }

    pub fn fen(fen: impl Into<String>) -> Self {
        Self { fen: Some(fen.into()), moves: Vec::new() }
    }

    pub fn with_moves<I, S>(mut self, moves: I) -> Self
    where
        I: IntoIterator<Item = S>,
        S: Into<String>,
    {
        self.moves.extend(moves.into_iter().map(Into::into));
        self
    }

    /// The `position` command.
    pub fn command(&self) -> String {
        let mut c = match &self.fen {
            None => "position startpos".to_owned(),
            Some(fen) if fen == "startpos" => "position startpos".to_owned(),
            Some(fen) => format!("position fen {fen}"),
        };
        if !self.moves.is_empty() {
            c += " moves ";
            c += &self.moves.join(" ");
        }
        c
    }
}

/// The clocks of a game, for `go wtime btime winc binc`.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Clock {
    pub wtime_ms: u64,
    pub btime_ms: u64,
    pub winc_ms: u64,
    pub binc_ms: u64,
    pub movestogo: Option<u32>,
}

/// How long to search. Several limits may be combined (the engine stops at the first reached); none at all is
/// `go infinite`, which runs until [`Engine::stop`].
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Limits {
    pub depth: Option<u32>,
    pub nodes: Option<u64>,
    pub movetime_ms: Option<u64>,
    pub clock: Option<Clock>,
    pub infinite: bool,
}

impl Limits {
    pub fn depth(depth: u32) -> Self {
        Self { depth: Some(depth), ..Self::default() }
    }

    pub fn nodes(nodes: u64) -> Self {
        Self { nodes: Some(nodes), ..Self::default() }
    }

    pub fn movetime(ms: u64) -> Self {
        Self { movetime_ms: Some(ms), ..Self::default() }
    }

    pub fn clock(clock: Clock) -> Self {
        Self { clock: Some(clock), ..Self::default() }
    }

    pub fn infinite() -> Self {
        Self { infinite: true, ..Self::default() }
    }

    /// The `go` command.
    pub fn command(&self) -> String {
        let mut go = String::from("go");
        if let Some(c) = self.clock {
            go += &format!(" wtime {} btime {} winc {} binc {}", c.wtime_ms, c.btime_ms, c.winc_ms, c.binc_ms);
            if let Some(m) = c.movestogo {
                go += &format!(" movestogo {m}");
            }
        }
        if let Some(d) = self.depth {
            go += &format!(" depth {d}");
        }
        if let Some(n) = self.nodes {
            go += &format!(" nodes {n}");
        }
        if let Some(t) = self.movetime_ms {
            go += &format!(" movetime {t}");
        }
        if self.infinite || go == "go" {
            go += " infinite";
        }
        go
    }
}

/// An option the engine declares in its `uci` answer.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct UciOption {
    pub name: String,
    /// `spin`, `check`, `combo`, `button` or `string`.
    pub kind: String,
    pub default: Option<String>,
    pub min: Option<i64>,
    pub max: Option<i64>,
    /// A combo's choices.
    pub vars: Vec<String>,
}

impl UciOption {
    /// Parses `option name <name…> type <kind> [default <v>] [min <n>] [max <n>] [var <v>]…`. Names may have spaces.
    pub fn parse(line: &str) -> Option<Self> {
        let rest = line.strip_prefix("option name ")?;
        let (name, rest) = rest.split_once(" type ")?;
        let mut words = rest.split_whitespace().peekable();
        let kind = words.next()?.to_owned();
        let mut option =
            UciOption { name: name.trim().to_owned(), kind, default: None, min: None, max: None, vars: Vec::new() };
        let keys = ["default", "min", "max", "var"];
        while let Some(key) = words.next() {
            let mut value = Vec::new();
            while let Some(w) = words.peek() {
                if keys.contains(w) {
                    break;
                }
                value.push(words.next()?);
            }
            let value = value.join(" ");
            match key {
                "default" => option.default = Some(value),
                "min" => option.min = value.parse().ok(),
                "max" => option.max = value.parse().ok(),
                "var" => option.vars.push(value),
                _ => {}
            }
        }
        Some(option)
    }
}

/// What a search ended with: the move, and the newest report of each line.
#[derive(Clone, Debug, PartialEq)]
pub struct SearchResult {
    pub best: BestMove,
    /// The newest `info` of each MultiPV line, line 1 first. A line reported at a lower depth than the first (the
    /// search stopped mid-iteration) is still the best the engine has for it.
    pub lines: Vec<Info>,
}

impl SearchResult {
    /// Total nodes searched: the largest count any line reported.
    pub fn nodes(&self) -> u64 {
        self.lines.iter().map(|i| i.nodes).max().unwrap_or(0)
    }
}

/// Keeps the newest report of each MultiPV line.
pub fn record_line(lines: &mut Vec<Info>, info: Info) {
    if info.multipv == 0 {
        return;
    }
    let i = info.multipv as usize - 1;
    if lines.len() <= i {
        lines.resize(i + 1, info.clone());
    }
    lines[i] = info;
}

/// A running engine process.
pub struct Engine {
    child: proc::Child,
    lines: mpsc::UnboundedReceiver<String>,
    /// `id name`.
    pub name: String,
    /// `id author`.
    pub author: String,
    /// The options it declared.
    pub options: Vec<UciOption>,
    searching: bool,
}

impl Engine {
    /// Starts the engine and waits for `uciok`. An engine that does not answer within [`UCIOK_TIMEOUT`] fails the call
    /// and is killed.
    pub async fn start(path: &Path) -> Result<Self> {
        Self::start_command(&proc::Command::new(path), UCIOK_TIMEOUT).await
    }

    /// Starts any command as an engine (arguments, environment), with a timeout for `uciok`.
    pub async fn start_command(command: &proc::Command, timeout: Duration) -> Result<Self> {
        let command = command.clone().stdin(true);
        let mut child = proc::spawn(&command)?;
        let lines = proc::lines(&mut child);
        let mut engine =
            Self { child, lines, name: String::new(), author: String::new(), options: Vec::new(), searching: false };
        engine.send("uci").await?;
        let handshake = async {
            while let Some(line) = engine.lines.recv().await {
                if let Some(name) = line.strip_prefix("id name ") {
                    engine.name = name.trim().to_owned();
                } else if let Some(author) = line.strip_prefix("id author ") {
                    engine.author = author.trim().to_owned();
                } else if let Some(option) = UciOption::parse(&line) {
                    engine.options.push(option);
                } else if line.trim() == "uciok" {
                    return Ok(());
                }
            }
            bail!("{} exited before uciok", command.program.display())
        };
        match tokio::time::timeout(timeout, handshake).await {
            Ok(Ok(())) => Ok(engine),
            Ok(Err(e)) => Err(e),
            Err(_) => bail!("{} sent no uciok within {} s", command.program.display(), timeout.as_secs_f64()),
        }
    }

    /// Sends one command. A line break inside it (from a FEN or a move in a request) would smuggle in another, so it is
    /// refused.
    pub async fn send(&mut self, command: &str) -> Result<()> {
        if command.contains(['\n', '\r']) {
            bail!("a UCI command cannot contain a line break");
        }
        let stdin = self.child.stdin().as_mut().context("engine stdin closed")?;
        stdin.write_all(format!("{command}\n").as_bytes()).await.context("write to the engine")?;
        stdin.flush().await.context("write to the engine")?;
        Ok(())
    }

    pub async fn set_option(&mut self, name: &str, value: impl std::fmt::Display) -> Result<()> {
        self.send(&format!("setoption name {name} value {value}")).await
    }

    /// Sets several options, then waits for `readyok`.
    pub async fn configure(&mut self, options: &[(String, String)]) -> Result<()> {
        for (name, value) in options {
            self.set_option(name, value).await?;
        }
        self.ready().await
    }

    /// `isready`, waiting for `readyok`. An engine that does not answer within [`READYOK_TIMEOUT`] fails the call; the
    /// caller drops it, which kills it.
    pub async fn ready(&mut self) -> Result<()> {
        self.send("isready").await?;
        let answer = async {
            while let Some(line) = self.lines.recv().await {
                if line.trim() == "readyok" {
                    return Ok(());
                }
            }
            bail!("engine exited before readyok")
        };
        tokio::time::timeout(READYOK_TIMEOUT, answer)
            .await
            .unwrap_or_else(|_| bail!("engine sent no readyok within {} s", READYOK_TIMEOUT.as_secs()))
    }

    /// `ucinewgame` and `isready`: forget the hash table and history, as before every game.
    pub async fn new_game(&mut self) -> Result<()> {
        self.send("ucinewgame").await?;
        self.ready().await
    }

    /// Sets the position and starts a search. Read its output with [`Engine::next`].
    pub async fn go(&mut self, position: &Position, limits: &Limits) -> Result<()> {
        self.go_command(position, &limits.command()).await
    }

    /// Like [`Engine::go`] with the whole `go` command given, for limits `Limits` does not carry.
    pub async fn go_command(&mut self, position: &Position, go: &str) -> Result<()> {
        if self.searching {
            bail!("the engine is already searching");
        }
        self.send(&position.command()).await?;
        self.send(go).await?;
        self.searching = true;
        Ok(())
    }

    /// The next event of a running search. After the `bestmove` the engine is idle again.
    pub async fn next(&mut self) -> Result<SearchEvent> {
        while let Some(line) = self.lines.recv().await {
            if let Some(best) = BestMove::parse(&line) {
                self.searching = false;
                return Ok(SearchEvent::BestMove(best));
            }
            if let Some(info) = Info::parse(&line) {
                return Ok(SearchEvent::Info(info));
            }
        }
        self.searching = false;
        bail!("the engine exited during the search")
    }

    /// The next raw line the engine writes, for anything this module does not parse (`info string`, diagnostics).
    pub async fn read_line(&mut self) -> Result<String> {
        self.lines.recv().await.context("the engine exited")
    }

    /// Whether a search is running (started and its `bestmove` not yet read).
    pub fn searching(&self) -> bool {
        self.searching
    }

    /// Asks a running search to stop. Its `bestmove` still has to be read ([`Engine::next`] or [`Engine::finish`]).
    pub async fn stop(&mut self) -> Result<()> {
        if self.searching { self.send("stop").await } else { Ok(()) }
    }

    /// Stops a running search and reads up to its `bestmove`, so the next search starts clean. Does nothing when idle.
    pub async fn finish(&mut self, timeout: Duration) -> Result<Option<BestMove>> {
        if !self.searching {
            return Ok(None);
        }
        self.stop().await?;
        let drain = async {
            loop {
                if let SearchEvent::BestMove(b) = self.next().await? {
                    return anyhow::Ok(b);
                }
            }
        };
        match tokio::time::timeout(timeout, drain).await {
            Ok(b) => b.map(Some),
            Err(_) => bail!("the engine did not stop within {} s", timeout.as_secs_f64()),
        }
    }

    /// Runs a whole search, handing each `info` to `on_info`, and returns the move and the newest report of each line.
    pub async fn search(
        &mut self,
        position: &Position,
        limits: &Limits,
        mut on_info: impl FnMut(&Info),
    ) -> Result<SearchResult> {
        self.go(position, limits).await?;
        let mut lines = Vec::new();
        loop {
            match self.next().await? {
                SearchEvent::Info(info) => {
                    on_info(&info);
                    record_line(&mut lines, info);
                }
                SearchEvent::BestMove(best) => return Ok(SearchResult { best, lines }),
            }
        }
    }

    /// Whether the process is still running.
    pub fn alive(&mut self) -> bool {
        !self.child.exited()
    }

    /// Sends `quit` and gives the engine a moment to exit before its tree is killed.
    pub async fn quit(mut self) {
        let _ = self.send("quit").await;
        let _ = tokio::time::timeout(Duration::from_millis(500), self.child.wait()).await;
    }

    /// Kills the engine at once.
    pub async fn kill(&mut self) {
        let _ = self.child.kill_tree().await;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_info_lines() {
        let line = "info depth 10 seldepth 13 multipv 1 score cp 42 nodes 15690 time 61 nps 254110 hashfull 4 tbhits 0 pv e2e4 e7e6";
        let info = Info::parse(line).unwrap();
        assert_eq!((info.depth, info.seldepth, info.multipv), (10, Some(13), 1));
        assert_eq!(info.score, Score::Cp(42));
        assert_eq!((info.nodes, info.nps, info.time_ms, info.hashfull), (15690, 254110, 61, Some(4)));
        assert_eq!(info.bound, Bound::Exact);
        assert_eq!(info.pv, vec!["e2e4", "e7e6"]);
        assert_eq!(Info::parse("info depth 3 score mate -2 pv a1a2").unwrap().score, Score::Mate(-2));
        assert_eq!(Info::parse("info string NNUE loaded"), None);
        assert_eq!(Info::parse("info depth 5 currmove e2e4 currmovenumber 1"), None);
    }

    #[test]
    fn reads_bounds_and_lines_without_a_pv() {
        let info = Info::parse("info depth 12 multipv 2 score cp -15 upperbound nodes 100 time 3").unwrap();
        assert_eq!((info.multipv, info.score, info.bound), (2, Score::Cp(-15), Bound::Upper));
        assert!(info.pv.is_empty());
        assert_eq!(Info::parse("info depth 1 score cp 5 lowerbound pv e2e4").unwrap().bound, Bound::Lower);
        assert_eq!(Info::parse("info depth 0 score mate 0").unwrap().score, Score::Mate(0));
    }

    #[test]
    fn parses_bestmove_lines() {
        assert_eq!(
            BestMove::parse("bestmove e2e4 ponder e7e5"),
            Some(BestMove { best: Some("e2e4".into()), ponder: Some("e7e5".into()) })
        );
        assert_eq!(BestMove::parse("bestmove e7e8q"), Some(BestMove { best: Some("e7e8q".into()), ponder: None }));
        assert_eq!(BestMove::parse("bestmove (none)"), Some(BestMove { best: None, ponder: None }));
        assert_eq!(BestMove::parse("bestmove 0000"), Some(BestMove { best: None, ponder: None }));
        assert_eq!(BestMove::parse("info depth 1"), None);
    }

    #[test]
    fn builds_go_commands() {
        assert_eq!(Limits::default().command(), "go infinite");
        assert_eq!(Limits::infinite().command(), "go infinite");
        assert_eq!(Limits::depth(12).command(), "go depth 12");
        assert_eq!(Limits::nodes(200_000).command(), "go nodes 200000");
        assert_eq!(Limits::movetime(1000).command(), "go movetime 1000");
        let clock = Clock { wtime_ms: 60_000, btime_ms: 55_000, winc_ms: 1000, binc_ms: 1000, movestogo: None };
        assert_eq!(Limits::clock(clock).command(), "go wtime 60000 btime 55000 winc 1000 binc 1000");
        assert_eq!(Limits { depth: Some(5), nodes: Some(10), ..Limits::default() }.command(), "go depth 5 nodes 10");
    }

    #[test]
    fn builds_position_commands() {
        assert_eq!(Position::startpos().command(), "position startpos");
        assert_eq!(Position::startpos().with_moves(["e2e4", "e7e5"]).command(), "position startpos moves e2e4 e7e5");
        assert_eq!(
            Position::fen("8/8/8/8/8/8/8/K1k5 w - - 0 1").command(),
            "position fen 8/8/8/8/8/8/8/K1k5 w - - 0 1"
        );
        assert_eq!(Position::fen("startpos").command(), "position startpos");
    }

    #[test]
    fn parses_option_declarations() {
        let o = UciOption::parse("option name Hash type spin default 16 min 1 max 262144").unwrap();
        assert_eq!((o.name.as_str(), o.kind.as_str()), ("Hash", "spin"));
        assert_eq!((o.default.as_deref(), o.min, o.max), (Some("16"), Some(1), Some(262144)));
        let o = UciOption::parse("option name Clear Hash type button").unwrap();
        assert_eq!((o.name.as_str(), o.kind.as_str(), o.default), ("Clear Hash", "button", None));
        let o = UciOption::parse("option name Style type combo default Solid var Solid var Risky Play").unwrap();
        assert_eq!(o.vars, vec!["Solid", "Risky Play"]);
        let o = UciOption::parse("option name SyzygyPath type string default <empty>").unwrap();
        assert_eq!(o.default.as_deref(), Some("<empty>"));
    }

    #[test]
    fn keeps_the_newest_report_of_each_line() {
        let mut lines = Vec::new();
        let info =
            |multipv, depth| Info::parse(&format!("info depth {depth} multipv {multipv} score cp 1 pv e2e4")).unwrap();
        record_line(&mut lines, info(1, 1));
        record_line(&mut lines, info(2, 1));
        record_line(&mut lines, info(1, 2));
        assert_eq!(lines.iter().map(|i| (i.multipv, i.depth)).collect::<Vec<_>>(), vec![(1, 2), (2, 1)]);
    }

    /// A program that runs and never says `uciok`: the start fails within the timeout and the program is killed.
    #[tokio::test]
    async fn an_engine_that_never_answers_fails_to_start() {
        #[cfg(windows)]
        let command = proc::Command::new("ping").args(["-n", "30", "127.0.0.1"]);
        #[cfg(not(windows))]
        let command = proc::Command::new("sleep").arg("60");
        let started = std::time::Instant::now();
        let result = Engine::start_command(&command, Duration::from_millis(500)).await;
        let error = result.err().expect("no uciok, no engine").to_string();
        assert!(error.contains("no uciok"), "{error}");
        assert!(started.elapsed() < Duration::from_secs(10));
    }

    #[tokio::test]
    async fn refuses_commands_with_line_breaks() {
        #[cfg(windows)]
        let command = proc::Command::new("cmd").args(["/C", "echo uciok & ping -n 30 127.0.0.1 > nul"]);
        #[cfg(not(windows))]
        let command = proc::Command::new("sh").args(["-c", "echo uciok; sleep 30"]);
        let mut engine = Engine::start_command(&command, Duration::from_secs(5)).await.unwrap();
        let err = engine.send("position startpos\ngo infinite").await.unwrap_err().to_string();
        assert!(err.contains("line break"), "{err}");
        engine.kill().await;
    }
}
