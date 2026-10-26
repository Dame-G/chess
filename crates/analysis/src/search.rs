//! From the engine's words to the domain's: UCI scores are the side to move's, every score analysis stores or sends is
//! White's. Everything that reads an engine here goes through these functions, so the flip happens in one place.

use std::path::Path;

use dameless_core::{
    Error, Score, Side,
    events::EvalLine,
    review::{EvalLine as PositionLine, PositionEval},
};
use dameless_engine::{
    pool::EngineSpec,
    uci::{self, Engine, Info, Limits, Position, SearchEvent},
};

/// Hash for review searches, in MiB: small, since each position is searched afresh from its bare FEN.
pub const REVIEW_HASH: u32 = 16;

/// Moves of a line kept in the position cache.
const PV_KEPT: usize = 12;

/// The side to move of a FEN (its second field).
pub fn side_to_move(fen: &str) -> Side {
    match fen.split_whitespace().nth(1) {
        Some("b") => Side::Black,
        _ => Side::White,
    }
}

/// A UCI score, from `to_move`'s point of view, as White's.
pub fn white_score(score: uci::Score, to_move: Side) -> Score {
    let own = match score {
        uci::Score::Cp(cp) => Score::Cp(cp),
        uci::Score::Mate(m) => Score::Mate(m),
    };
    own.from_side(to_move)
}

/// The engines a review (or [`crate::Eval::evaluate`]) searches with: one thread each, a small hash, `multipv` lines.
pub fn review_spec(binary: &Path, multipv: u32) -> EngineSpec {
    EngineSpec::new(binary).option("Threads", 1).option("Hash", REVIEW_HASH).option("MultiPV", multipv.max(1))
}

/// Keeps the newest report of each line asked for. An engine's `multipv` counts from 1; a line it numbers beyond what
/// was asked for is ignored.
pub fn record(lines: &mut Vec<Info>, info: Info, multipv: u32) {
    if info.multipv == 0 || info.multipv > multipv.max(1) {
        return;
    }
    uci::record_line(lines, info);
}

/// Searches one position, given as a bare FEN (no history, as the review always has), to `nodes`, and returns its
/// lines with White's scores, the engine's own move first.
pub async fn search_fen(engine: &mut Engine, fen: &str, nodes: u64, multipv: u32) -> Result<PositionEval, Error> {
    engine.go(&Position::fen(fen), &Limits::nodes(nodes)).await.map_err(|e| Error::internal(format!("{e:#}")))?;
    let mut infos: Vec<Info> = Vec::new();
    let best = loop {
        match engine.next().await.map_err(|e| Error::internal(format!("{e:#}")))? {
            SearchEvent::Info(info) => record(&mut infos, info, multipv),
            SearchEvent::BestMove(best) => break best.best,
        }
    };
    let Some(best) = best.filter(|_| !infos.is_empty()) else {
        return Err(Error::internal(format!("the engine found no move at {fen}")));
    };
    Ok(position_eval(infos, &best, side_to_move(fen)))
}

/// The lines of a finished search as the cache keeps them: White's scores, the engine's move first.
pub fn position_eval(infos: Vec<Info>, best: &str, to_move: Side) -> PositionEval {
    // A line reported at a lower depth than the best one (the search stopped mid-iteration) still counts: it is the
    // best the engine has for it.
    let depth = infos.first().map_or(0, |i| i.depth);
    let nodes = infos.iter().map(|i| i.nodes).max().unwrap_or(0);
    let mut lines: Vec<PositionLine> = infos
        .into_iter()
        .filter(|i| !i.pv.is_empty())
        .map(|i| PositionLine {
            uci: i.pv[0].clone(),
            score: white_score(i.score, to_move),
            pv: i.pv.into_iter().take(PV_KEPT).collect(),
        })
        .collect();
    // The move the engine plays is the best line, whatever the last report said.
    if let Some(i) = lines.iter().position(|l| l.uci == best)
        && i != 0
    {
        lines.swap(0, i);
    }
    PositionEval { lines, depth, nodes }
}

/// The lines of a live search for `eval.info`: White's scores.
pub fn live_lines(infos: &[Info], to_move: Side) -> Vec<EvalLine> {
    infos.iter().map(|i| EvalLine { score: white_score(i.score, to_move), pv: i.pv.clone() }).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn info(multipv: u32, score: &str, pv: &str) -> Info {
        Info::parse(&format!("info depth 9 multipv {multipv} score {score} nodes 500 pv {pv}")).unwrap()
    }

    #[test]
    fn scores_become_whites() {
        assert_eq!(side_to_move("8/8/8/8/8/8/8/K1k5 b - - 0 1"), Side::Black);
        assert_eq!(side_to_move("startpos"), Side::White);
        assert_eq!(white_score(uci::Score::Cp(40), Side::Black), Score::Cp(-40));
        assert_eq!(white_score(uci::Score::Mate(-2), Side::Black), Score::Mate(2));
        assert_eq!(white_score(uci::Score::Cp(40), Side::White), Score::Cp(40));
    }

    #[test]
    fn the_engines_move_comes_first_and_lines_beyond_multipv_are_ignored() {
        let mut lines = Vec::new();
        record(&mut lines, info(1, "cp 30", "e7e5 g1f3"), 2);
        record(&mut lines, info(2, "cp 10", "c7c5"), 2);
        record(&mut lines, info(3, "cp 0", "a7a6"), 2);
        let eval = position_eval(lines, "c7c5", Side::Black);
        assert_eq!(eval.lines.len(), 2);
        assert_eq!((eval.lines[0].uci.as_str(), eval.lines[0].score), ("c7c5", Score::Cp(-10)));
        assert_eq!(eval.lines[1].score, Score::Cp(-30));
        assert_eq!((eval.depth, eval.nodes), (9, 500));
    }
}
