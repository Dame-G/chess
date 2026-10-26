//! Running a match with fastchess and reading its progress.

use std::path::Path;

use anyhow::{Result, bail};
use tokio_util::sync::CancellationToken;

use crate::{
    funnel::{MatchSpec, TimeControl},
    proc,
    stats::Pentanomial,
};

pub struct Match<'a> {
    pub fastchess: &'a Path,
    pub book: &'a Path,
    pub dev: &'a Path,
    pub base: &'a Path,
    /// UCI options for one engine only, such as `rfp_quad=1140` (SPSA's two sides of a perturbation).
    pub dev_options: &'a [(String, String)],
    pub base_options: &'a [(String, String)],
    pub spec: &'a MatchSpec,
    pub concurrency: u32,
    pub pgn: &'a Path,
    pub cancel: &'a CancellationToken,
    pub log: &'a (dyn Fn(&str) + Sync),
    /// Called with the running totals at each rating interval.
    pub progress: &'a (dyn Fn(Pentanomial) + Sync),
}

/// SPRT tests play until the SPRT decides; this many pairs is only a ceiling.
const MAX_PAIRS: u32 = 200_000;

pub fn args(m: &Match) -> Vec<String> {
    let s = m.spec;
    let path = |p: &Path| p.display().to_string();
    let mut each = vec![
        "proto=uci".to_owned(),
        format!("option.Hash={}", s.hash),
        format!("option.Threads={}", s.threads),
        "option.Minimal=true".to_owned(),
        "option.MoveOverhead=0".to_owned(),
    ];
    match s.tc {
        TimeControl::Clock { seconds, increment } => {
            each.push(format!("tc={seconds}+{increment}"));
            // A 250 ms margin, as OpenBench gives: with MoveOverhead=0, pipe latency alone would lose games on time.
            each.push("timemargin=250".to_owned());
        }
        TimeControl::Nodes { nodes } => each.push(format!("nodes={nodes}")),
    }

    let options = |o: &[(String, String)]| o.iter().map(|(k, v)| format!("option.{k}={v}")).collect::<Vec<_>>();
    let mut args = vec!["-engine".into(), format!("cmd={}", path(m.dev)), "name=dev".into()];
    args.extend(options(m.dev_options));
    args.extend(["-engine".into(), format!("cmd={}", path(m.base)), "name=base".into()]);
    args.extend(options(m.base_options));
    args.push("-each".into());
    args.extend(each);
    args.extend([
        "-openings".into(),
        format!("file={}", path(m.book)),
        "format=epd".into(),
        "order=random".into(),
        "-repeat".into(),
        "-games".into(),
        "2".into(),
        "-rounds".into(),
        s.pairs.unwrap_or(MAX_PAIRS).to_string(),
        "-concurrency".into(),
        m.concurrency.to_string(),
        "-ratinginterval".into(),
        "10".into(),
        "-pgnout".into(),
        format!("file={}", path(m.pgn)),
        "nodes=true".into(),
        "seldepth=true".into(),
        "nps=true".into(),
        // Reckless's adjudication: decided games end early, dead draws too.
        "-resign".into(),
        "movecount=3".into(),
        "score=400".into(),
        "-draw".into(),
        "movenumber=30".into(),
        "movecount=6".into(),
        "score=10".into(),
    ]);
    if let Some(sprt) = s.sprt {
        args.extend([
            "-sprt".into(),
            format!("elo0={}", sprt.elo0),
            format!("elo1={}", sprt.elo1),
            format!("alpha={}", sprt.alpha),
            format!("beta={}", sprt.beta),
            "model=normalized".into(),
        ]);
    }
    args
}

/// Plays the match to the end (SPRT decision or all pairs) and returns the final game-pair counts.
pub async fn run(m: Match<'_>) -> Result<Pentanomial> {
    let args = args(&m);
    let refs: Vec<&str> = args.iter().map(String::as_str).collect();
    (m.log)(&format!("fastchess {}", args.join(" ")));
    let mut child = proc::spawn(proc::Spawn { program: m.fastchess, args: &refs, cwd: None, env: &[], stdin: false })?;
    let mut lines = proc::lines(&mut child);
    let mut totals = Pentanomial::default();
    let mut tail: Vec<String> = Vec::new();
    loop {
        tokio::select! {
            line = lines.recv() => match line {
                Some(line) => {
                    if let Some(p) = parse_ptnml(&line) {
                        totals = p;
                        (m.progress)(p);
                    }
                    // Per-game lines are in the PGN; the log keeps the summaries.
                    if !line.starts_with("Started game") && !line.starts_with("Finished game") {
                        (m.log)(&line);
                        tail.push(line);
                        if tail.len() > 20 {
                            tail.remove(0);
                        }
                    }
                }
                None => break,
            },
            _ = m.cancel.cancelled() => bail!("cancelled"),
        }
    }
    let status = child.wait().await?;
    if !status.success() {
        bail!("fastchess exited with {status}: {}", tail.join(" | "));
    }
    Ok(totals)
}

/// `Ptnml(0-2): [LL, LD, WL+DD, WD, WW], WL/DD Ratio: ...`
fn parse_ptnml(line: &str) -> Option<Pentanomial> {
    let inner = line.strip_prefix("Ptnml(0-2): [")?.split(']').next()?;
    let mut counts = [0u64; 5];
    let mut parts = inner.split(',');
    for c in &mut counts {
        *c = parts.next()?.trim().parse().ok()?;
    }
    Some(Pentanomial(counts))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::funnel::{Preset, bounds};

    #[test]
    fn parses_pentanomial_lines() {
        assert_eq!(parse_ptnml("Ptnml(0-2): [1, 1, 3, 1, 2], WL/DD Ratio: 2.00"), Some(Pentanomial([1, 1, 3, 1, 2])));
        assert_eq!(parse_ptnml("LLR: 0.02 (0.6%) (-2.25, 2.89) [0.00, 3.00]"), None);
    }

    #[test]
    fn builds_reckless_style_arguments() {
        let spec = Preset::Stc.spec(Some(bounds(0.0, 3.0)), None);
        let token = CancellationToken::new();
        let args = args(&Match {
            fastchess: Path::new("fc"),
            book: Path::new("book.epd"),
            dev: Path::new("dev"),
            base: Path::new("base"),
            dev_options: &[("rfp_quad".into(), "1200".into())],
            base_options: &[],
            spec: &spec,
            concurrency: 14,
            pgn: Path::new("games.pgn"),
            cancel: &token,
            log: &|_| {},
            progress: &|_| {},
        });
        let joined = args.join(" ");
        assert!(joined.contains("tc=8+0.08"), "{joined}");
        assert!(joined.contains("option.Hash=16 option.Threads=1 option.Minimal=true option.MoveOverhead=0"));
        assert!(joined.contains("-sprt elo0=0 elo1=3 alpha=0.05 beta=0.1 model=normalized"));
        assert!(joined.contains("-concurrency 14"));
        assert!(joined.contains("name=dev option.rfp_quad=1200 -engine cmd=base name=base -each"), "{joined}");
    }
}
