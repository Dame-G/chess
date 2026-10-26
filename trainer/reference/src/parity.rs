//! Parity with the engine: Reckless's own evaluations of a list of positions, compared with the reference's.
//!
//! Reckless's non-UCI `eval` command refreshes the network from scratch and prints the network's output for every
//! output bucket and for the bucket the position uses, each as `raw / 100` from White's point of view with two
//! decimals. `raw` is an integer of at most a few thousand, so the printed value gives it back exactly. No patch to
//! the engine is needed.

use std::io::{BufRead, BufReader, Write};
use std::path::Path;
use std::process::{Command, Stdio};

use anyhow::{Context, bail, ensure};
use shakmaty::{CastlingMode, Chess, Color, Position, fen::Fen};

use crate::arch::OUTPUT_BUCKETS;
use crate::network::Network;

/// What the engine printed for one position, from the side to move.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct EngineEval {
    /// "NNUE evaluation": the network through the position's own bucket.
    pub eval: i32,
    /// The network through each output bucket.
    pub buckets: [i32; OUTPUT_BUCKETS],
}

/// Parses a FEN as the reference does (castling in either notation).
pub fn parse_fen(fen: &str) -> anyhow::Result<Chess> {
    let parsed: Fen = fen.parse().with_context(|| format!("parsing FEN {fen:?}"))?;
    parsed.into_position(CastlingMode::Chess960).map_err(|e| anyhow::anyhow!("illegal position {fen:?}: {e}"))
}

/// `"+1.08"` → 108: the engine prints `raw as f32 / 100.0` with two decimals.
fn centi(text: &str) -> anyhow::Result<i32> {
    let value: f64 = text.trim().parse().with_context(|| format!("reading the value {text:?}"))?;
    Ok((value * 100.0).round() as i32)
}

/// Runs `engine` once over all `fens` (`position fen …` then `eval` for each, in one process) and returns what it
/// printed, from each position's side to move.
pub fn engine_evals(engine: &Path, fens: &[String]) -> anyhow::Result<Vec<EngineEval>> {
    let mut child = Command::new(engine)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::inherit())
        .spawn()
        .with_context(|| format!("starting {}", engine.display()))?;

    let mut stdin = child.stdin.take().context("the engine's stdin")?;
    let commands: Vec<String> = fens.iter().map(|fen| format!("position fen {fen}\neval\n")).collect();
    let writer = std::thread::spawn(move || -> std::io::Result<()> {
        for command in commands {
            stdin.write_all(command.as_bytes())?;
        }
        stdin.write_all(b"quit\n")?;
        stdin.flush()
    });

    let stdout = BufReader::new(child.stdout.take().context("the engine's stdout")?);
    let mut results = Vec::with_capacity(fens.len());
    let mut buckets: Vec<i32> = Vec::new();

    for line in stdout.lines() {
        let line = line.context("reading the engine's output")?;

        // A bucket row: `|  >   3      |   +1.08    |` or `|      0      |   +2.48    |`.
        let cells: Vec<&str> = line.split('|').collect();
        if cells.len() == 4 && cells[0].is_empty() && cells[3].is_empty() {
            let label = cells[1].trim().trim_start_matches('>').trim();
            if let Ok(bucket) = label.parse::<usize>() {
                ensure!(bucket == buckets.len(), "bucket rows out of order: {line:?}");
                buckets.push(centi(cells[2])?);
            }
            continue;
        }

        if let Some(rest) = line.strip_prefix("NNUE evaluation") {
            let value = rest.split_whitespace().next().context("the evaluation's value")?;
            let index = results.len();
            let fen = fens.get(index).context("more evaluations than positions")?;
            let white_to_move = parse_fen(fen)?.turn() == Color::White;
            let sign = if white_to_move { 1 } else { -1 };

            ensure!(buckets.len() == OUTPUT_BUCKETS, "{} bucket rows before evaluation {index}", buckets.len());
            let mut per_bucket = [0; OUTPUT_BUCKETS];
            for (out, &value) in per_bucket.iter_mut().zip(&buckets) {
                *out = sign * value;
            }
            buckets.clear();
            results.push(EngineEval { eval: sign * centi(value)?, buckets: per_bucket });
        }
    }

    writer.join().expect("the writer thread").context("writing to the engine")?;
    let status = child.wait()?;
    if results.len() != fens.len() {
        bail!("the engine evaluated {} of {} positions (exit {status})", results.len(), fens.len());
    }
    Ok(results)
}

/// One position where the reference and the engine disagree.
#[derive(Clone, Debug)]
pub struct Mismatch {
    pub fen: String,
    pub engine: EngineEval,
    pub reference: EngineEval,
    /// Whether the first layer's 16-bit pairwise add wrapped (where AVX2, VNNI and scalar builds part ways).
    pub l1_wrapped: bool,
}

/// The outcome of a parity run.
#[derive(Clone, Debug, Default)]
pub struct Report {
    pub positions: usize,
    /// Positions whose evaluation and all eight per-bucket outputs match.
    pub matched: usize,
    /// Per-bucket outputs compared (positions × 8) and how many matched.
    pub bucket_values: usize,
    pub bucket_matched: usize,
    /// Positions per output bucket used, to show the run covered them all.
    pub per_bucket: [usize; OUTPUT_BUCKETS],
    /// Positions where any bucket's first-layer sums wrapped in 16 bits.
    pub l1_wrapped: usize,
    pub mismatches: Vec<Mismatch>,
}

impl Report {
    pub fn passed(&self) -> bool {
        self.positions > 0 && self.matched == self.positions
    }
}

/// The reference's view of a position, in the engine's terms.
pub fn reference_eval(network: &Network, position: &Chess) -> (EngineEval, bool) {
    let accumulators = network.accumulators(position);
    let mut buckets = [0; OUTPUT_BUCKETS];
    let mut wrapped = false;
    for (bucket, out) in buckets.iter_mut().enumerate() {
        let forward = network.propagate(accumulators.clone(), bucket);
        wrapped |= forward.l1_wrapped;
        *out = forward.eval;
    }
    let eval = buckets[crate::features::output_bucket(position.board())];
    (EngineEval { eval, buckets }, wrapped)
}

/// Compares the reference with the engine's printed evaluations of `fens`.
pub fn compare(network: &Network, fens: &[String], engine: &[EngineEval]) -> anyhow::Result<Report> {
    ensure!(fens.len() == engine.len(), "{} positions but {} engine evaluations", fens.len(), engine.len());
    let mut report = Report { positions: fens.len(), ..Report::default() };

    for (fen, engine) in fens.iter().zip(engine) {
        let position = parse_fen(fen)?;
        let (reference, wrapped) = reference_eval(network, &position);
        report.per_bucket[crate::features::output_bucket(position.board())] += 1;
        report.l1_wrapped += wrapped as usize;
        report.bucket_values += OUTPUT_BUCKETS;
        report.bucket_matched += reference.buckets.iter().zip(&engine.buckets).filter(|(a, b)| a == b).count();

        if reference == *engine {
            report.matched += 1;
        } else {
            report.mismatches.push(Mismatch {
                fen: fen.clone(),
                engine: engine.clone(),
                reference,
                l1_wrapped: wrapped,
            });
        }
    }
    Ok(report)
}

/// Writes engine evaluations as a fixture: one line per position, `<eval> <bucket 0> … <bucket 7> <fen>`, after a
/// header of `#` comments.
pub fn write_expected(path: &Path, header: &str, fens: &[String], evals: &[EngineEval]) -> anyhow::Result<()> {
    let mut out = String::new();
    for line in header.lines() {
        out.push_str(&format!("# {line}\n"));
    }
    for (fen, eval) in fens.iter().zip(evals) {
        let buckets: Vec<String> = eval.buckets.iter().map(i32::to_string).collect();
        out.push_str(&format!("{} {} {fen}\n", eval.eval, buckets.join(" ")));
    }
    std::fs::write(path, out).with_context(|| format!("writing {}", path.display()))
}

/// Reads a fixture written by [`write_expected`].
pub fn read_expected(text: &str) -> anyhow::Result<Vec<(String, EngineEval)>> {
    let mut entries = Vec::new();
    for line in text.lines().map(str::trim).filter(|l| !l.is_empty() && !l.starts_with('#')) {
        let mut fields = line.splitn(1 + 1 + OUTPUT_BUCKETS, ' ');
        let mut number = || -> anyhow::Result<i32> {
            let field = fields.next().context("a short line")?;
            field.parse().with_context(|| format!("reading {field:?} in {line:?}"))
        };
        let eval = number()?;
        let mut buckets = [0; OUTPUT_BUCKETS];
        for bucket in &mut buckets {
            *bucket = number()?;
        }
        let fen = fields.next().context("a line without a FEN")?.to_string();
        entries.push((fen, EngineEval { eval, buckets }));
    }
    Ok(entries)
}
