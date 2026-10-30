//! Themes: what a mistake of yours was about, found from the position and the engine's line, never guessed. A mistake
//! gets exactly one theme, the first of these that its evidence supports, so counts and costs add up:
//!
//! 1. [`ThemeKind::TimeTrouble`]: you had under a tenth of the starting clock (the review's own time-trouble rule).
//! 2. [`ThemeKind::KingSafety`]: your move allowed a forced mate, or moved the king and gave up castling before the
//!    endgame.
//! 3. [`ThemeKind::HangingPiece`]: after your move the opponent could win material by capture (static exchange, net of
//!    what you captured) and the engine's move would not have allowed it.
//! 4. [`ThemeKind::MissedTactic`]: the engine's line wins material within four plies, or mates, and you missed it.
//! 5. [`ThemeKind::OpeningError`] and [`ThemeKind::EndgameTechnique`]: nothing above, and the position is in that
//!    phase (lila's Divider, as the review reads it).
//! 6. [`ThemeKind::Other`]: everything else (a quiet middlegame mistake). It is shown as "other", not guessed at.
//!
//! Pins, skewers, deflections and positional ideas are not detected: they cannot be told from the engine's line by a
//! cheap rule without calling too many mistakes by the wrong name. The detail is a short phrase for the cause
//! ("a knight", "fork", "mate in 3"), always from the same evidence as the kind.

use serde::{Deserialize, Serialize};
use shakmaty::{Bitboard, Chess, Color, Move, Position as _, Role, Square};
use utoipa::ToSchema;

use crate::{chess::Position, ids::Side, review::judge::Phase, score::Score};

/// What a mistake was about.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum ThemeKind {
    /// The mistake came with under a tenth of the starting clock left.
    TimeTrouble,
    /// A forced mate was allowed, or the king left and castling was given up.
    KingSafety,
    /// A piece was left where it could be taken for less than it is worth.
    HangingPiece,
    /// The engine's line wins material or mates, and it was missed.
    MissedTactic,
    /// A mistake in the opening phase.
    OpeningError,
    /// A mistake in the endgame phase.
    EndgameTechnique,
    /// None of the above.
    #[default]
    Other,
}

impl ThemeKind {
    /// Every kind, most specific first: the order themes are tried in, and `Other` last.
    pub const ALL: [Self; 7] = [
        Self::TimeTrouble,
        Self::KingSafety,
        Self::HangingPiece,
        Self::MissedTactic,
        Self::OpeningError,
        Self::EndgameTechnique,
        Self::Other,
    ];

    /// The name in the API, a query and the database.
    pub fn word(self) -> &'static str {
        match self {
            Self::TimeTrouble => "time_trouble",
            Self::KingSafety => "king_safety",
            Self::HangingPiece => "hanging_piece",
            Self::MissedTactic => "missed_tactic",
            Self::OpeningError => "opening_error",
            Self::EndgameTechnique => "endgame_technique",
            Self::Other => "other",
        }
    }

    pub fn from_word(word: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|k| k.word() == word)
    }
}

/// A theme found: its kind and, when the evidence names one, the cause.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
pub struct Detected {
    pub kind: ThemeKind,
    /// `a knight`, `fork`, `mate in 3`, `3 s left`: a short phrase, lower case.
    pub detail: Option<String>,
}

impl Detected {
    fn new(kind: ThemeKind, detail: impl Into<Option<String>>) -> Self {
        Self { kind, detail: detail.into() }
    }
}

/// What the finder reads about one mistake.
pub struct Mistake<'a> {
    /// The position before the move.
    pub before: &'a Position,
    /// The move played and the engine's best, UCI.
    pub played: &'a str,
    pub best: &'a str,
    /// The engine's line after the position, its best move first (UCI).
    pub best_pv: &'a [String],
    /// The engine's scores before and after the move, White's point of view.
    pub eval_before: Score,
    pub eval_after: Score,
    pub phase: Phase,
    /// The mover's clock before the move, and the starting clock, in a game with a clock.
    pub clock_ms: Option<u64>,
    pub initial_ms: Option<u64>,
}

/// Plies of the engine's line read for material won.
const LINE_PLIES: usize = 4;
/// Pawns a hanging piece costs at least, net: a minor piece for a pawn is 2.
const HANGS_AT_LEAST: i32 = 2;
/// Pawns the engine's line must win for a tactic to be missed.
const WINS_AT_LEAST: i32 = 2;

/// The theme of one mistake. Anything that cannot be read (a move that is not legal there) leaves it `Other`.
pub fn detect(m: &Mistake<'_>) -> Detected {
    let side = m.before.turn();
    if let (Some(clock), Some(initial)) = (m.clock_ms, m.initial_ms)
        && clock * 10 < initial
    {
        return Detected::new(ThemeKind::TimeTrouble, format!("{} s left", clock / 1000));
    }
    let (Ok(played), Ok(best)) = (m.before.uci_move(m.played), m.before.uci_move(m.best)) else {
        return phase_theme(m.phase);
    };
    if let Some(king) = king_safety(m, side, &played) {
        return king;
    }
    if let Some(hang) = hanging(m.before.inner(), &played, &best) {
        return hang;
    }
    if let Some(tactic) = missed_tactic(m, side, &best) {
        return tactic;
    }
    phase_theme(m.phase)
}

fn phase_theme(phase: Phase) -> Detected {
    match phase {
        Phase::Opening => Detected::new(ThemeKind::OpeningError, None),
        Phase::Endgame => Detected::new(ThemeKind::EndgameTechnique, None),
        Phase::Middlegame => Detected::default(),
    }
}

fn king_safety(m: &Mistake<'_>, side: Side, played: &Move) -> Option<Detected> {
    if let Score::Mate(n) = m.eval_after.for_side(side)
        && n < 0
        && !matches!(m.eval_before.for_side(side), Score::Mate(b) if b < 0)
    {
        return Some(Detected::new(ThemeKind::KingSafety, format!("mate in {}", -n)));
    }
    let board = m.before.inner();
    let color = Color::from(side);
    let castles = board.castles().castling_rights().intersects(board.board().by_color(color));
    if m.phase != Phase::Endgame && played.role() == Role::King && !played.is_castle() && castles {
        return Some(Detected::new(ThemeKind::KingSafety, "king moved, castling lost".to_owned()));
    }
    None
}

fn value(role: Role) -> i32 {
    match role {
        Role::Pawn => 1,
        Role::Knight | Role::Bishop => 3,
        Role::Rook => 5,
        Role::Queen => 9,
        Role::King => 100,
    }
}

fn name(role: Role) -> &'static str {
    match role {
        Role::Pawn => "a pawn",
        Role::Knight => "a knight",
        Role::Bishop => "a bishop",
        Role::Rook => "a rook",
        Role::Queen => "a queen",
        Role::King => "the king",
    }
}

fn after(pos: &Chess, m: &Move) -> Chess {
    let mut p = pos.clone();
    p.play_unchecked(*m);
    p
}

/// The static exchange of a capture, in pawns: what the side to move nets by starting the exchange on the capture's
/// square and both sides recapturing with their least valuable piece while it pays. Pins are not read.
fn see(pos: &Chess, m: &Move) -> i32 {
    let board = pos.board();
    let to = m.to();
    let Some(from) = m.from() else { return 0 };
    let captured = if m.is_en_passant() { Role::Pawn } else { m.capture().unwrap_or(Role::Pawn) };
    let mut gain = [0i32; 40];
    gain[0] = value(captured);
    let mut occupied = board.occupied().without(from);
    let mut on_square = value(m.promotion().unwrap_or_else(|| m.role()));
    let mut side = pos.turn();
    let mut depth = 0usize;
    loop {
        depth += 1;
        if depth >= gain.len() - 1 {
            break;
        }
        gain[depth] = on_square - gain[depth - 1];
        if (-gain[depth - 1]).max(gain[depth]) < 0 {
            break;
        }
        side = side.other();
        let attackers = (board.attacks_to(to, Color::White, occupied) | board.attacks_to(to, Color::Black, occupied))
            & occupied
            & board.by_color(side);
        let Some((sq, role)) = least_valuable(pos, attackers) else { break };
        occupied = occupied.without(sq);
        on_square = value(role);
    }
    while depth > 1 {
        depth -= 1;
        gain[depth - 1] = -((-gain[depth - 1]).max(gain[depth]));
    }
    gain[0]
}

fn least_valuable(pos: &Chess, attackers: Bitboard) -> Option<(Square, Role)> {
    attackers.into_iter().filter_map(|sq| pos.board().role_at(sq).map(|r| (sq, r))).min_by_key(|(_, r)| value(*r))
}

/// The best capture the side to move has: what it nets (static exchange) and the piece it takes.
fn best_capture(pos: &Chess) -> Option<(i32, Role)> {
    pos.legal_moves()
        .iter()
        .filter(|m| m.is_capture())
        .map(|m| (see(pos, m), if m.is_en_passant() { Role::Pawn } else { m.capture().unwrap_or(Role::Pawn) }))
        .max_by_key(|(g, r)| (*g, value(*r)))
}

/// What the side that just moved gains on the move itself: the value of the piece it took.
fn took(m: &Move) -> i32 {
    if m.is_en_passant() { 1 } else { m.capture().map_or(0, value) }
}

fn hanging(pos: &Chess, played: &Move, best: &Move) -> Option<Detected> {
    if played == best {
        return None;
    }
    let net = |m: &Move| {
        let next = after(pos, m);
        best_capture(&next).map_or((0, None), |(g, r)| (g.max(0) - took(m), Some(r)))
    };
    let (lost_played, piece) = net(played);
    let (lost_best, _) = net(best);
    if lost_played >= HANGS_AT_LEAST && lost_played >= lost_best + HANGS_AT_LEAST {
        return Some(Detected::new(ThemeKind::HangingPiece, piece.map(|r| name(r).to_owned())));
    }
    None
}

fn missed_tactic(m: &Mistake<'_>, side: Side, best: &Move) -> Option<Detected> {
    if let Score::Mate(n) = m.eval_before.for_side(side)
        && n > 0
    {
        return Some(Detected::new(ThemeKind::MissedTactic, format!("mate in {n}")));
    }
    let start = m.before.inner();
    let color = Color::from(side);
    let balance = |p: &Chess| crate::chess::balance(p, color);
    let mut line = Position::from_chess(start.clone());
    let mut plies = 0;
    for uci in m.best_pv.iter().take(LINE_PLIES + 2) {
        let Ok(mv) = line.uci_move(uci) else { break };
        line.play_move(mv);
        plies += 1;
        // Stop at four plies, or a little later while a capture is still hanging in the air.
        if plies >= LINE_PLIES && !mv.is_capture() {
            break;
        }
    }
    if plies == 0 || balance(line.inner()) - balance(start) < WINS_AT_LEAST {
        return None;
    }
    let detail = if is_fork(start, best) {
        "fork"
    } else if best.is_capture() {
        "winning capture"
    } else if after(start, best).is_check() {
        "check"
    } else {
        "wins material"
    };
    Some(Detected::new(ThemeKind::MissedTactic, detail.to_owned()))
}

/// Whether a quiet move forks: the moved piece attacks two or more enemy pieces that are worth more than it or loose,
/// and the opponent cannot just take it.
fn is_fork(pos: &Chess, m: &Move) -> bool {
    if m.is_capture() || m.is_castle() {
        return false;
    }
    let next = after(pos, m);
    let to = m.to();
    let board = next.board();
    let them = next.turn();
    let me = value(board.role_at(to).unwrap_or(m.role()));
    let targets = (board.attacks_from(to) & board.by_color(them))
        .into_iter()
        .filter(|sq| {
            let role = board.role_at(*sq).unwrap_or(Role::Pawn);
            let defended = board.attacks_to(*sq, them, board.occupied()).without(*sq).any();
            role == Role::King || value(role) > me || !defended
        })
        .filter(|sq| board.role_at(*sq) != Some(Role::Pawn) || m.role() == Role::Pawn)
        .count();
    if targets < 2 {
        return false;
    }
    // A fork that loses the forking piece for nothing is no fork.
    let takes = next.legal_moves().iter().filter(|c| c.is_capture() && c.to() == to).map(|c| see(&next, c)).max();
    takes.is_none_or(|g| g <= 0)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn mistake<'a>(pos: &'a Position, played: &'a str, best: &'a str, pv: &'a [String]) -> Mistake<'a> {
        Mistake {
            before: pos,
            played,
            best,
            best_pv: pv,
            eval_before: Score::Cp(0),
            eval_after: Score::Cp(-300),
            phase: Phase::Middlegame,
            clock_ms: None,
            initial_ms: None,
        }
    }

    fn pv(moves: &[&str]) -> Vec<String> {
        moves.iter().map(|m| (*m).to_owned()).collect()
    }

    fn kind(pos: &str, played: &str, best: &str, line: &[&str], tweak: impl FnOnce(&mut Mistake<'_>)) -> Detected {
        let pos = Position::from_fen(pos).unwrap();
        let line = pv(line);
        let mut m = mistake(&pos, played, best, &line);
        tweak(&mut m);
        detect(&m)
    }

    #[test]
    fn the_static_exchange_counts_recaptures() {
        // White's knight takes a pawn defended by a pawn: wins 1, loses 3, so −2 in all.
        let p = Position::from_fen("4k3/8/3p4/4p3/8/5N2/8/4K3 w - - 0 1").unwrap();
        let nxe5 = p.uci_move("f3e5").unwrap();
        assert_eq!(see(p.inner(), &nxe5), -2);
        // Undefended pawn: free.
        let p = Position::from_fen("4k3/8/8/4p3/8/5N2/8/4K3 w - - 0 1").unwrap();
        assert_eq!(see(p.inner(), &p.uci_move("f3e5").unwrap()), 1);
        // A defended knight taken by a pawn nets +3 − 0.
        let p = Position::from_fen("4k3/8/8/3n4/4P3/8/8/4K3 w - - 0 1").unwrap();
        assert_eq!(see(p.inner(), &p.uci_move("e4d5").unwrap()), 3);
    }

    #[test]
    fn a_short_clock_is_time_trouble_whatever_else_it_was() {
        let d = kind("4k3/8/8/8/8/8/8/R3K3 w - - 0 1", "a1a2", "a1a8", &[], |m| {
            m.clock_ms = Some(14_000);
            m.initial_ms = Some(180_000);
        });
        assert_eq!(d, Detected { kind: ThemeKind::TimeTrouble, detail: Some("14 s left".into()) });
        // Exactly a tenth is not under it.
        let d = kind("4k3/8/8/8/8/8/8/R3K3 w - - 0 1", "a1a2", "a1a8", &[], |m| {
            m.clock_ms = Some(18_000);
            m.initial_ms = Some(180_000);
            m.phase = Phase::Endgame;
        });
        assert_eq!(d.kind, ThemeKind::EndgameTechnique);
    }

    #[test]
    fn an_allowed_mate_and_a_lost_castle_are_king_safety() {
        let d = kind("rnbqkbnr/pppp1ppp/8/4p3/4P3/8/PPPP1PPP/RNBQKBNR w KQkq - 0 2", "e1e2", "g1f3", &[], |m| {
            m.eval_after = Score::Mate(-3);
        });
        assert_eq!(d, Detected { kind: ThemeKind::KingSafety, detail: Some("mate in 3".into()) });
        // From Black's point of view a White-positive mate is for the mover.
        let d = kind("rnbqkbnr/pppp1ppp/8/4p3/4P3/5N2/PPPP1PPP/RNBQKB1R b KQkq - 1 2", "e8e7", "b8c6", &[], |m| {
            m.eval_after = Score::Cp(-100);
            m.phase = Phase::Opening;
        });
        assert_eq!(d, Detected { kind: ThemeKind::KingSafety, detail: Some("king moved, castling lost".into()) });
        // A castling move is not that, and a king move without castling rights is not either.
        let d = kind("4k3/8/8/8/8/8/8/R3K3 w - - 0 1", "e1d2", "a1a8", &[], |m| m.phase = Phase::Endgame);
        assert_eq!(d.kind, ThemeKind::EndgameTechnique);
    }

    #[test]
    fn leaving_a_piece_to_be_taken_is_a_hanging_piece() {
        // White plays Bb5 where the bishop is taken by the a6 pawn... here: the knight on f3 steps to d4 where a pawn
        // takes it; Nxe5 (best) was safe.
        let d = kind("rnbqkbnr/pppp1ppp/8/4p3/8/5N2/PPPPPPPP/RNBQKB1R w KQkq - 0 1", "f3d4", "f3e5", &["f3e5"], |_| {});
        // d4 is attacked by the e5 pawn: the knight is lost for a pawn at best.
        assert_eq!(d, Detected { kind: ThemeKind::HangingPiece, detail: Some("a knight".into()) });
        // An even trade is not hanging: Nxe5 Nxe5 loses nothing net once the pawn is counted, and the best move
        // walks into the same capture.
        let d = kind("4k3/8/8/4p3/8/3n1N2/8/4K3 w - - 0 1", "f3e5", "f3e5", &[], |_| {});
        assert_ne!(d.kind, ThemeKind::HangingPiece);
    }

    #[test]
    fn a_hanging_piece_that_the_best_move_leaves_too_is_not_the_cause() {
        // The queen on d1 is attacked by the bishop on g4 whatever White does with the a-pawn.
        let d = kind("4k3/8/8/8/6b1/8/P7/3QK3 w - - 0 1", "a2a3", "a2a4", &[], |m| m.phase = Phase::Endgame);
        assert_eq!(d.kind, ThemeKind::EndgameTechnique);
    }

    #[test]
    fn a_line_that_wins_material_is_a_missed_tactic() {
        // The knight forks king and rook: Nc7+ wins the rook on a8.
        let d = kind("r3k3/8/8/3N4/8/8/8/4K3 w - - 0 1", "e1e2", "d5c7", &["d5c7", "e8d7", "c7a8"], |_| {});
        assert_eq!(d, Detected { kind: ThemeKind::MissedTactic, detail: Some("fork".into()) });
        // A capture that wins a piece.
        let d = kind("4k3/8/8/3q4/8/8/3R4/3RK3 w - - 0 1", "e1f2", "d2d5", &["d2d5"], |_| {});
        assert_eq!(d, Detected { kind: ThemeKind::MissedTactic, detail: Some("winning capture".into()) });
        // The engine saw a mate.
        let d = kind("6k1/5ppp/8/8/8/8/8/R5K1 w - - 0 1", "g1f1", "a1a8", &["a1a8"], |m| {
            m.eval_before = Score::Mate(1);
        });
        assert_eq!(d, Detected { kind: ThemeKind::MissedTactic, detail: Some("mate in 1".into()) });
        // Black's mate is a mate for the mover: Mate(-1) is White's loss.
        let d = kind("6k1/5ppp/8/8/8/8/r7/6K1 b - - 0 1", "a2a1", "a2a1", &["a2a1"], |m| {
            m.eval_before = Score::Mate(-1);
        });
        assert_eq!(d.detail.as_deref(), Some("mate in 1"));
    }

    #[test]
    fn quiet_mistakes_are_named_by_their_phase_or_other() {
        let q =
            |phase| kind("4k3/8/8/8/8/8/4P3/4K3 w - - 0 1", "e2e3", "e2e4", &["e2e4"], move |m| m.phase = phase).kind;
        assert_eq!(q(Phase::Opening), ThemeKind::OpeningError);
        assert_eq!(q(Phase::Endgame), ThemeKind::EndgameTechnique);
        assert_eq!(q(Phase::Middlegame), ThemeKind::Other);
        // Moves that are not legal there read as the phase.
        assert_eq!(
            kind("4k3/8/8/8/8/8/4P3/4K3 w - - 0 1", "zz", "e2e4", &[], |m| m.phase = Phase::Opening).kind,
            ThemeKind::OpeningError
        );
    }

    #[test]
    fn kinds_have_words_that_read_back() {
        for k in ThemeKind::ALL {
            assert_eq!(ThemeKind::from_word(k.word()), Some(k));
        }
        assert_eq!(ThemeKind::from_word("pins"), None);
    }
}
