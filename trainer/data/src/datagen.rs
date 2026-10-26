//! Self-play data generation, the way Reckless's `datagen` branch is meant to be used: an engine built from that
//! branch plays itself at a fixed soft node count (`UseSoftNodes`: the node limit is checked where the time manager
//! would stop, at the end of an iteration, never in the middle of one), from DFRC, Chess960, standard or book starts
//! followed by random plies, with adjudication. Games are written as viriformat.
//!
//! The run is split over `threads` workers, each with its own engine process (one search thread each) and its own
//! output file (`part-<worker>.vf`). It stops at a target number of positions or when the files would pass the disk
//! cap, whichever comes first, or when asked to stop.
//!
//! **Resumable.** `datagen.json` in the output directory records, for each part, the bytes (and positions and games)
//! written as whole games and flushed. It is rewritten every couple of seconds and at the end. A run started on a
//! directory that has one carries on: each part is first cut back to its recorded length (dropping a game half
//! written when the last run was killed), then appended to. Killing the process is therefore always safe. At the end
//! `manifest.json` is (re)built.

use std::{
    fs::OpenOptions,
    io::Write,
    path::{Path, PathBuf},
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, Ordering},
    },
    time::{Duration, Instant},
};

use anyhow::{Context, Result, anyhow, bail};
use serde::{Deserialize, Serialize};
use shakmaty::{Chess, Color, EnPassantMode, Position, zobrist::Zobrist64};

use crate::{
    formats::{pgn::ScoreUnits, viri},
    game::{self, Entry, Game, GameResult, MATE_SCORE},
    manifest,
    openings::{Opening, Openings},
    rng::Rng,
    uci::{self, Engine, Score},
};

pub const STATE_FILE: &str = "datagen.json";

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
#[serde(default)]
pub struct Adjudication {
    /// A side wins once every score for `win_plies` plies in a row is at least this, in its favour (UCI cp).
    pub win_score: i32,
    pub win_plies: u32,
    /// A draw once every score for `draw_plies` plies in a row is within this, after `draw_after_ply` plies.
    pub draw_score: i32,
    pub draw_plies: u32,
    pub draw_after_ply: u32,
    /// A draw at this many plies of play.
    pub max_plies: u32,
}

impl Default for Adjudication {
    fn default() -> Self {
        Self { win_score: 1000, win_plies: 4, draw_score: 10, draw_plies: 12, draw_after_ply: 60, max_plies: 400 }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
#[serde(default)]
pub struct DatagenConfig {
    /// The engine binary, built from Reckless's `datagen` branch (any UCI engine works; without `UseSoftNodes` the
    /// node count is a hard limit).
    pub engine: PathBuf,
    pub threads: usize,
    /// Stop once this many positions are written (a few games past it, one per worker at most).
    pub target_positions: u64,
    /// Never let the parts grow past this many bytes.
    pub max_bytes: u64,
    /// The node count per move (soft, with `UseSoftNodes`).
    pub soft_nodes: u64,
    pub hash_mb: u32,
    pub opening: Opening,
    /// Random legal plies played from the opening before the engine takes over.
    pub random_plies: u32,
    /// Openings whose score (searched at `opening_nodes`) is beyond this are thrown away (0: never checked).
    pub max_opening_score: i32,
    pub opening_nodes: u64,
    pub adjudication: Adjudication,
    pub seed: u64,
    pub units: ScoreUnits,
    /// A search that does not answer in this long fails the game (and the engine is restarted).
    pub move_timeout_ms: u64,
}

impl Default for DatagenConfig {
    fn default() -> Self {
        Self {
            engine: PathBuf::new(),
            threads: 1,
            target_positions: 1_000_000,
            max_bytes: 10 * 1024 * 1024 * 1024,
            soft_nodes: 5000,
            hash_mb: 16,
            opening: Opening::Dfrc,
            random_plies: 8,
            max_opening_score: 1000,
            opening_nodes: 5000,
            adjudication: Adjudication::default(),
            seed: 0,
            units: ScoreUnits::Cp,
            move_timeout_ms: 30_000,
        }
    }
}

#[derive(Clone, Debug, Default, Serialize, Deserialize, PartialEq)]
pub struct PartState {
    pub file: String,
    pub bytes: u64,
    pub positions: u64,
    pub games: u64,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct Session {
    pub started: String,
    pub config: DatagenConfig,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct State {
    pub parts: Vec<PartState>,
    pub sessions: Vec<Session>,
    pub updated: String,
    /// How the last session ended: `target`, `disk-cap`, `stopped`, `failed`, or empty while running.
    pub finished: String,
}

impl State {
    pub fn positions(&self) -> u64 {
        self.parts.iter().map(|p| p.positions).sum()
    }
    pub fn games(&self) -> u64 {
        self.parts.iter().map(|p| p.games).sum()
    }
    pub fn bytes(&self) -> u64 {
        self.parts.iter().map(|p| p.bytes).sum()
    }

    pub fn load(dir: &Path) -> Result<Option<Self>> {
        let path = dir.join(STATE_FILE);
        if !path.exists() {
            return Ok(None);
        }
        let text = std::fs::read_to_string(&path)?;
        Ok(Some(serde_json::from_str(&text).with_context(|| format!("parse {}", path.display()))?))
    }

    fn save(&self, dir: &Path) -> Result<()> {
        manifest::write_json_atomic(&dir.join(STATE_FILE), self)
    }
}

#[derive(Clone, Debug, Serialize)]
pub struct Progress {
    pub positions: u64,
    pub games: u64,
    pub bytes: u64,
    pub target_positions: u64,
    pub max_bytes: u64,
    /// Positions written in this session, and per second.
    pub session_positions: u64,
    pub positions_per_second: f64,
    pub elapsed_seconds: f64,
}

#[derive(Clone, Debug, Serialize)]
pub struct Report {
    pub positions: u64,
    pub games: u64,
    pub bytes: u64,
    pub session_positions: u64,
    pub session_games: u64,
    pub elapsed_seconds: f64,
    pub positions_per_second: f64,
    pub bytes_per_position: f64,
    /// `target`, `disk-cap` or `stopped`.
    pub finished: String,
    /// Engine failures (timeouts, crashes, illegal moves) that cost a game.
    pub failures: u64,
}

struct Shared {
    state: State,
    /// Positions and bytes of games being written, already counted against the caps.
    reserved_positions: u64,
    reserved_bytes: u64,
    session_positions: u64,
    session_games: u64,
    failures: u64,
    finished: Option<String>,
}

/// Runs datagen into `dir` until the target, the disk cap or `stop`. `progress` is called every couple of seconds.
pub fn run(
    config: &DatagenConfig,
    dir: &Path,
    stop: &AtomicBool,
    progress: &(dyn Fn(&Progress) + Sync),
) -> Result<Report> {
    if config.threads == 0 {
        bail!("datagen needs at least one thread");
    }
    if !config.engine.is_file() {
        bail!("no engine at {}", config.engine.display());
    }
    std::fs::create_dir_all(dir)?;
    let openings = Openings::new(&config.opening)?;

    let mut state = State::load(dir)?.unwrap_or_default();
    // Cut every part back to what was recorded as written.
    for part in &state.parts {
        let path = dir.join(&part.file);
        let len = std::fs::metadata(&path).map(|m| m.len()).unwrap_or(0);
        if len < part.bytes {
            bail!("{} is shorter ({len} bytes) than {STATE_FILE} records ({})", path.display(), part.bytes);
        }
        if len > part.bytes {
            OpenOptions::new().write(true).open(&path)?.set_len(part.bytes)?;
        }
    }
    for worker in 0..config.threads {
        let file = part_name(worker);
        if !state.parts.iter().any(|p| p.file == file) {
            state.parts.push(PartState { file, ..Default::default() });
        }
    }
    state.sessions.push(Session { started: manifest::now_rfc3339(), config: config.clone() });
    state.finished.clear();
    state.save(dir)?;

    let shared = Arc::new(Mutex::new(Shared {
        state,
        reserved_positions: 0,
        reserved_bytes: 0,
        session_positions: 0,
        session_games: 0,
        failures: 0,
        finished: None,
    }));
    let started = Instant::now();
    let done = AtomicBool::new(false);

    let snapshot = |shared: &Shared| -> Progress {
        let elapsed = started.elapsed().as_secs_f64();
        Progress {
            positions: shared.state.positions(),
            games: shared.state.games(),
            bytes: shared.state.bytes(),
            target_positions: config.target_positions,
            max_bytes: config.max_bytes,
            session_positions: shared.session_positions,
            positions_per_second: if elapsed > 0.0 { shared.session_positions as f64 / elapsed } else { 0.0 },
            elapsed_seconds: elapsed,
        }
    };

    let worker_errors: Vec<anyhow::Error> = std::thread::scope(|scope| {
        let mut handles = Vec::new();
        for worker in 0..config.threads {
            let shared = Arc::clone(&shared);
            let openings = &openings;
            let done = &done;
            handles.push(scope.spawn(move || worker_loop(worker, config, dir, openings, &shared, stop, done)));
        }
        // The monitor: progress and the state file, until every worker is done.
        let mut last_save = Instant::now();
        while handles.iter().any(|h| !h.is_finished()) {
            std::thread::sleep(Duration::from_millis(100));
            if last_save.elapsed() >= Duration::from_secs(2) {
                last_save = Instant::now();
                let s = shared.lock().unwrap();
                let _ = s.state.save(dir);
                let p = snapshot(&s);
                drop(s);
                progress(&p);
            }
        }
        handles.into_iter().filter_map(|h| h.join().unwrap_or_else(|_| Err(anyhow!("worker panicked"))).err()).collect()
    });

    let mut s = shared.lock().unwrap();
    let finished = s
        .finished
        .clone()
        .unwrap_or_else(|| if stop.load(Ordering::Relaxed) { "stopped".into() } else { "failed".into() });
    s.state.finished = finished.clone();
    s.state.updated = manifest::now_rfc3339();
    s.state.save(dir)?;
    let p = snapshot(&s);
    progress(&p);
    let report = Report {
        positions: p.positions,
        games: p.games,
        bytes: p.bytes,
        session_positions: s.session_positions,
        session_games: s.session_games,
        elapsed_seconds: p.elapsed_seconds,
        positions_per_second: p.positions_per_second,
        bytes_per_position: if p.positions > 0 { p.bytes as f64 / p.positions as f64 } else { 0.0 },
        finished: finished.clone(),
        failures: s.failures,
    };
    drop(s);

    let generator = serde_json::json!({ "tool": "data datagen", "config": config });
    manifest::build_and_write(
        dir,
        &manifest::ManifestOptions {
            name: None,
            source: Some("datagen".into()),
            generator: Some(generator),
            score_units: Some(config.units),
        },
    )?;

    if finished == "failed" {
        let message = worker_errors.iter().map(|e| format!("{e:#}")).collect::<Vec<_>>().join("; ");
        bail!("datagen failed: {message}");
    }
    Ok(report)
}

pub fn part_name(worker: usize) -> String {
    format!("part-{worker:03}.vf")
}

fn worker_loop(
    worker: usize,
    config: &DatagenConfig,
    dir: &Path,
    openings: &Openings,
    shared: &Mutex<Shared>,
    stop: &AtomicBool,
    done: &AtomicBool,
) -> Result<()> {
    let file_name = part_name(worker);
    let mut file = OpenOptions::new().create(true).append(true).open(dir.join(&file_name))?;
    let games_before = shared.lock().unwrap().state.parts.iter().find(|p| p.file == file_name).map_or(0, |p| p.games);
    let mut engine: Option<Engine> = None;
    let mut consecutive_failures = 0;
    let mut game_index = games_before;
    loop {
        if stop.load(Ordering::Relaxed) || done.load(Ordering::Relaxed) {
            return Ok(());
        }
        if engine.is_none() {
            engine = Some(start_engine(config)?);
        }
        let seed = config.seed ^ ((worker as u64) << 40) ^ game_index.wrapping_mul(0x9E37_79B9_7F4A_7C15);
        let mut rng = Rng::new(seed);
        game_index += 1;
        let game = match play_game(engine.as_mut().unwrap(), config, openings, &mut rng, stop) {
            Ok(Some(game)) => game,
            Ok(None) => return Ok(()), // stopped
            Err(e) => {
                consecutive_failures += 1;
                shared.lock().unwrap().failures += 1;
                if let Some(e) = engine.take() {
                    e.quit();
                }
                if consecutive_failures >= 5 {
                    return Err(e.context(format!("worker {worker}: five failures in a row")));
                }
                continue;
            }
        };
        consecutive_failures = 0;
        if game.is_empty() {
            continue;
        }
        let bytes = viri::encode_game(&game);
        let positions = game.len() as u64;
        {
            let mut s = shared.lock().unwrap();
            if s.finished.is_some() {
                return Ok(());
            }
            let total_positions = s.state.positions() + s.reserved_positions;
            let total_bytes = s.state.bytes() + s.reserved_bytes;
            if total_positions >= config.target_positions {
                s.finished = Some("target".into());
                done.store(true, Ordering::Relaxed);
                return Ok(());
            }
            if total_bytes + bytes.len() as u64 > config.max_bytes {
                s.finished = Some("disk-cap".into());
                done.store(true, Ordering::Relaxed);
                return Ok(());
            }
            s.reserved_positions += positions;
            s.reserved_bytes += bytes.len() as u64;
        }
        let written = file.write_all(&bytes).and_then(|_| file.flush()).and_then(|_| file.sync_data());
        let mut s = shared.lock().unwrap();
        s.reserved_positions -= positions;
        s.reserved_bytes -= bytes.len() as u64;
        written.with_context(|| format!("write {file_name}"))?;
        let part = s.state.parts.iter_mut().find(|p| p.file == file_name).expect("part");
        part.bytes += bytes.len() as u64;
        part.positions += positions;
        part.games += 1;
        s.session_positions += positions;
        s.session_games += 1;
        if s.state.positions() >= config.target_positions && s.finished.is_none() {
            s.finished = Some("target".into());
            done.store(true, Ordering::Relaxed);
        }
    }
}

fn start_engine(config: &DatagenConfig) -> Result<Engine> {
    let mut engine = Engine::start(&config.engine)?;
    engine.set_option("Threads", 1)?;
    engine.set_option("Hash", config.hash_mb)?;
    for (name, value) in [("UCI_Chess960", "true"), ("UseSoftNodes", "true"), ("Minimal", "true")] {
        if engine.has_option(name) {
            engine.set_option(name, value)?;
        }
    }
    engine.ready()?;
    Ok(engine)
}

fn score_cp(score: Option<Score>) -> i32 {
    match score {
        Some(Score::Cp(cp)) => cp,
        Some(Score::Mate(n)) if n > 0 => i32::from(MATE_SCORE),
        Some(Score::Mate(_)) => -i32::from(MATE_SCORE),
        None => 0,
    }
}

/// Plays one game; `None` when stopped half way.
pub fn play_game(
    engine: &mut Engine,
    config: &DatagenConfig,
    openings: &Openings,
    rng: &mut Rng,
    stop: &AtomicBool,
) -> Result<Option<Game>> {
    let timeout = Duration::from_millis(config.move_timeout_ms);
    engine.new_game()?;
    let start = pick_opening(engine, config, openings, rng, timeout)?;
    let start_fen = game::fen(&start);
    let mut game = Game::new(start.clone(), GameResult::Draw);
    let mut pos: Chess = start;
    let mut moves: Vec<String> = Vec::new();
    let mut hashes: Vec<u64> = vec![hash(&pos)];
    let adj = &config.adjudication;
    let (mut white_streak, mut black_streak, mut draw_streak) = (0u32, 0u32, 0u32);

    let result = loop {
        if stop.load(Ordering::Relaxed) {
            return Ok(None);
        }
        let legal = pos.legal_moves();
        if legal.is_empty() {
            break if pos.is_check() { GameResult::from_winner(Some(!pos.turn())) } else { GameResult::Draw };
        }
        if pos.is_insufficient_material() || pos.halfmoves() >= 100 {
            break GameResult::Draw;
        }
        let current = *hashes.last().unwrap();
        if hashes.iter().filter(|&&h| h == current).count() >= 3 {
            break GameResult::Draw;
        }
        if game.len() as u32 >= adj.max_plies {
            break GameResult::Draw;
        }

        let search = engine.search(
            &uci::position_command(&start_fen, &moves),
            &format!("go nodes {}", config.soft_nodes),
            timeout,
        )?;
        let mv = game::parse_uci(&pos, &search.bestmove)?;
        let cp = score_cp(search.score);
        let score = if cp.abs() >= i32::from(MATE_SCORE) {
            cp.signum() as i16 * MATE_SCORE
        } else {
            config.units.convert(cp, &pos)
        };
        game.entries.push(Entry { mv, score });

        let white_cp = if pos.turn() == Color::White { cp } else { -cp };
        if white_cp >= adj.win_score {
            white_streak += 1;
            black_streak = 0;
        } else if white_cp <= -adj.win_score {
            black_streak += 1;
            white_streak = 0;
        } else {
            white_streak = 0;
            black_streak = 0;
        }
        if game.len() as u32 >= adj.draw_after_ply && cp.abs() <= adj.draw_score {
            draw_streak += 1;
        } else {
            draw_streak = 0;
        }
        if adj.win_plies > 0 && white_streak >= adj.win_plies {
            break GameResult::WhiteWins;
        }
        if adj.win_plies > 0 && black_streak >= adj.win_plies {
            break GameResult::BlackWins;
        }
        if adj.draw_plies > 0 && draw_streak >= adj.draw_plies {
            break GameResult::Draw;
        }

        moves.push(game::uci(mv));
        let zeroing = mv.is_zeroing();
        pos.play_unchecked(mv);
        if zeroing {
            hashes.clear();
        }
        hashes.push(hash(&pos));
    };
    game.result = result;
    Ok(Some(game))
}

fn hash(pos: &Chess) -> u64 {
    pos.zobrist_hash::<Zobrist64>(EnPassantMode::Legal).0
}

fn pick_opening(
    engine: &mut Engine,
    config: &DatagenConfig,
    openings: &Openings,
    rng: &mut Rng,
    timeout: Duration,
) -> Result<Chess> {
    for _ in 0..100 {
        let pos = openings.start(rng, config.random_plies)?;
        if config.max_opening_score <= 0 {
            return Ok(pos);
        }
        let search = engine.search(
            &uci::position_command(&game::fen(&pos), &[]),
            &format!("go nodes {}", config.opening_nodes.max(1)),
            timeout,
        )?;
        if score_cp(search.score).abs() <= config.max_opening_score {
            return Ok(pos);
        }
    }
    bail!("no opening within ±{} after 100 tries", config.max_opening_score)
}

/// Opens a part for reading back (tests, tools).
pub fn part_path(dir: &Path, worker: usize) -> PathBuf {
    dir.join(part_name(worker))
}
