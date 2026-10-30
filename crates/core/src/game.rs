//! Games: two seats, moves, clocks, a result, wherever the game was played. Local games, your Lichess games and
//! imported history are all this one record.

use serde::{Deserialize, Serialize};
use utoipa::ToSchema;

use crate::{
    ids::{ConnectionKey, GameId, PlayerKey, Side, SidePair, Timestamp},
    score::Score,
    time::{Speed, TimeControl},
};

/// Someone at a board, as this game knew them.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
pub struct Seat {
    pub player: PlayerKey,
    /// What to show: "You" is shown as the Lichess username when linked; an engine seat by the engine's name, or at a
    /// level of the ladder by its rating ("Stockfish 1650").
    pub name: String,
    /// The level of the ladder an engine seat played at (or Lichess's AI level); none at full strength.
    #[serde(default)]
    pub level: Option<u32>,
    /// What an engine seat's engine called itself (its UCI `id name`): `Stockfish 19`, `Reckless 0.9.0`.
    #[serde(default)]
    pub engine: Option<String>,
    pub rating: Option<u32>,
    pub title: Option<String>,
}

impl Seat {
    /// A seat with just a player and a name.
    pub fn new(player: PlayerKey, name: impl Into<String>) -> Self {
        Self { player, name: name.into(), level: None, engine: None, rating: None, title: None }
    }

    pub fn is_you(&self) -> bool {
        self.player == PlayerKey::You
    }
}

/// Where a game comes from.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum Source {
    /// Played here, or pasted in as PGN.
    Local,
    /// Played through your Lichess connection.
    Lichess { connection: ConnectionKey, id: String, rated: bool, speed: Speed },
    /// History read from Lichess: `owner` is whose history it is (you, or another player).
    Import { owner: PlayerKey, id: String, rated: bool, speed: Speed },
}

impl Source {
    /// The column the store indexes sources by.
    pub fn kind(&self) -> &'static str {
        match self {
            Self::Local => "local",
            Self::Lichess { .. } => "lichess",
            Self::Import { .. } => "import",
        }
    }

    /// The Lichess game id, for a game played or read there.
    pub fn lichess_id(&self) -> Option<&str> {
        match self {
            Self::Lichess { id, .. } | Self::Import { id, .. } => Some(id),
            _ => None,
        }
    }

    /// Whether the game counts for ratings on Lichess.
    pub fn rated(&self) -> bool {
        matches!(self, Self::Lichess { rated: true, .. } | Self::Import { rated: true, .. })
    }

    /// The game on lichess.org, for a game played or read there.
    pub fn url(&self, lichess: &str) -> Option<String> {
        self.lichess_id().map(|id| format!("{}/{id}", lichess.trim_end_matches('/')))
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum GameStatus {
    /// Set up, before the first move can be asked for (engines starting, Lichess not confirmed yet).
    Starting,
    Live,
    Finished,
    /// Stopped before it was decided: no result.
    Aborted,
}

/// Who won.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum Outcome {
    WhiteWins,
    BlackWins,
    Draw,
}

impl Outcome {
    pub fn won_by(side: Side) -> Self {
        match side {
            Side::White => Self::WhiteWins,
            Side::Black => Self::BlackWins,
        }
    }

    pub fn lost_by(side: Side) -> Self {
        Self::won_by(side.other())
    }

    pub fn winner(self) -> Option<Side> {
        match self {
            Self::WhiteWins => Some(Side::White),
            Self::BlackWins => Some(Side::Black),
            Self::Draw => None,
        }
    }

    /// 1 for a win, ½ for a draw, 0 for a loss, from `side`'s view.
    pub fn points(self, side: Side) -> f64 {
        match self.winner() {
            None => 0.5,
            Some(w) if w == side => 1.0,
            Some(_) => 0.0,
        }
    }

    /// The PGN result: `1-0`, `0-1`, `1/2-1/2`.
    pub fn pgn(self) -> &'static str {
        match self {
            Self::WhiteWins => "1-0",
            Self::BlackWins => "0-1",
            Self::Draw => "1/2-1/2",
        }
    }

    /// Reads a PGN result; `*` (unfinished) is none.
    pub fn parse_pgn(text: &str) -> Option<Self> {
        match text.trim() {
            "1-0" => Some(Self::WhiteWins),
            "0-1" => Some(Self::BlackWins),
            "1/2-1/2" | "½-½" => Some(Self::Draw),
            _ => None,
        }
    }
}

/// Why a game ended.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum Ending {
    Checkmate,
    /// A player resigned (or stopped a game of theirs that had got going).
    Resignation,
    /// A clock ran out. A draw when the other side could never have mated.
    Timeout,
    Stalemate,
    /// The same position for the third time.
    Repetition,
    /// Fifty moves each without a capture or a pawn move.
    FiftyMoves,
    InsufficientMaterial,
    /// The players agreed a draw.
    Agreement,
    /// Decided early by the tests' rules: one side was lost for several moves, or the score sat at zero.
    Adjudication,
    /// A player left and the other claimed the game or a draw (Lichess's `timeout`).
    Abandoned,
    /// Stopped before it got going, or from outside; no result.
    Aborted,
    /// An engine exited or stopped answering.
    EngineFailure,
    /// An engine answered with a move that is not legal.
    IllegalMove,
    /// Anything else Lichess reports (`noStart`, `cheat`, `variantEnd`, an unknown status).
    Other,
}

impl Ending {
    /// The PGN `Termination` header.
    pub fn termination(self) -> &'static str {
        match self {
            Self::Checkmate
            | Self::Stalemate
            | Self::Repetition
            | Self::FiftyMoves
            | Self::InsufficientMaterial
            | Self::Resignation
            | Self::Agreement
            | Self::Other => "normal",
            Self::Timeout => "time forfeit",
            Self::EngineFailure | Self::IllegalMove => "rules infraction",
            Self::Adjudication => "adjudication",
            Self::Aborted | Self::Abandoned => "abandoned",
        }
    }
}

/// How a game ended: the result (none when aborted) and why.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize, ToSchema)]
pub struct GameEnd {
    pub outcome: Option<Outcome>,
    pub ending: Ending,
}

impl GameEnd {
    pub fn new(outcome: Outcome, ending: Ending) -> Self {
        Self { outcome: Some(outcome), ending }
    }

    pub fn lost_by(side: Side, ending: Ending) -> Self {
        Self::new(Outcome::lost_by(side), ending)
    }

    pub fn draw(ending: Ending) -> Self {
        Self::new(Outcome::Draw, ending)
    }

    pub fn aborted() -> Self {
        Self { outcome: None, ending: Ending::Aborted }
    }

    /// The status a game with this end has.
    pub fn status(&self) -> GameStatus {
        if self.outcome.is_none() { GameStatus::Aborted } else { GameStatus::Finished }
    }

    /// The PGN result, `*` when there is none.
    pub fn pgn(&self) -> &'static str {
        self.outcome.map_or("*", Outcome::pgn)
    }
}

/// The opening as Lichess names it.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
pub struct Opening {
    /// `C20`.
    pub eco: Option<String>,
    /// `King's Pawn Game: Busch-Gass Gambit`.
    pub name: String,
    /// How many plies the named opening covers: moves up to it are `book` in a review.
    pub ply: Option<u32>,
}

impl Opening {
    /// The family: the name without the variation, `King's Pawn Game`.
    pub fn family(&self) -> String {
        opening_family(&self.name)
    }
}

/// `"C20 King's Pawn Game: Busch-Gass Gambit"` (or without the code) is the family `"King's Pawn Game"`.
pub fn opening_family(name: &str) -> String {
    opening_title(name).split(':').next().unwrap_or_default().trim().to_owned()
}

/// `"C20 King's Pawn Game: Busch-Gass Gambit"` (or without the code) is `"King's Pawn Game: Busch-Gass Gambit"`: the
/// opening's whole name, which names one line.
pub fn opening_title(name: &str) -> String {
    let mut words = name.splitn(2, ' ');
    let first = words.next().unwrap_or_default();
    let is_eco = first.len() == 3
        && first.starts_with(['A', 'B', 'C', 'D', 'E'])
        && first[1..].chars().all(|c| c.is_ascii_digit());
    let rest = if is_eco { words.next().unwrap_or_default() } else { name };
    rest.trim().to_owned()
}

/// What lists, events and the Games rail show of a game.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, ToSchema)]
pub struct GameSummary {
    pub id: GameId,
    pub white: Seat,
    pub black: Seat,
    pub source: Source,
    pub time: TimeControl,
    pub status: GameStatus,
    pub outcome: Option<Outcome>,
    pub ending: Option<Ending>,
    pub opening: Option<Opening>,
    pub plies: u32,
    pub started_at: Timestamp,
    pub ended_at: Option<Timestamp>,
    /// The side You played, if any.
    pub you: Option<Side>,
    /// Each side's accuracy from the newest review.
    pub accuracy: Option<SidePair<f32>>,
    /// The game on lichess.org.
    pub url: Option<String>,
}

impl GameSummary {
    pub fn seat(&self, side: Side) -> &Seat {
        match side {
            Side::White => &self.white,
            Side::Black => &self.black,
        }
    }

    /// The side `player` sat on, if they played.
    pub fn side_of(&self, player: &PlayerKey) -> Option<Side> {
        Side::BOTH.into_iter().find(|s| &self.seat(*s).player == player)
    }

    pub fn is_live(&self) -> bool {
        matches!(self.status, GameStatus::Starting | GameStatus::Live)
    }

    /// The speed the game was played at: Lichess's own for Lichess games, else from the clock.
    pub fn speed(&self) -> Option<Speed> {
        match &self.source {
            Source::Lichess { speed, .. } | Source::Import { speed, .. } => Some(*speed),
            _ => self.time.speed(),
        }
    }
}

/// One move of a game.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, ToSchema)]
pub struct Move {
    pub uci: String,
    pub san: String,
    /// The mover's clock after the move (increment included).
    pub clock_ms: Option<u64>,
    /// How long the mover thought.
    pub think_ms: Option<u64>,
    /// The mover's engine's own score, from White's point of view; engines only (and Lichess's analysis on imports).
    pub eval: Option<Score>,
    pub depth: Option<u32>,
    pub nodes: Option<u64>,
}

impl Move {
    /// A bare move, with nothing known but its notation.
    pub fn new(uci: impl Into<String>, san: impl Into<String>) -> Self {
        Self { uci: uci.into(), san: san.into(), clock_ms: None, think_ms: None, eval: None, depth: None, nodes: None }
    }
}

/// Offers standing in a live game: who offered a draw, who asked to take back.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
pub struct Offers {
    pub draw: Option<Side>,
    pub takeback: Option<Side>,
}

/// The full record of a game.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, ToSchema)]
pub struct Game {
    pub summary: GameSummary,
    /// The start position; none for the standard start.
    pub start_fen: Option<String>,
    pub moves: Vec<Move>,
    /// While live: the clock each side holds now, in ms.
    pub clocks: Option<SidePair<u64>>,
    pub offers: Offers,
    /// What You may do now; anything else is refused.
    pub actions: Vec<SeatAction>,
}

/// What a seat can do in a game. One set for every game, local or on Lichess; `Game.actions` lists those You may do
/// now. A move carries its UCI in the request, not here.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum SeatAction {
    Move,
    OfferDraw,
    AcceptDraw,
    DeclineDraw,
    /// Ask to take back your last move (and the reply, when there is one).
    Takeback,
    AcceptTakeback,
    DeclineTakeback,
    Resign,
    /// End a game that has not got going, with no result.
    Abort,
    /// Claim the game once the opponent has been gone long enough (Lichess).
    ClaimVictory,
    /// Claim a draw once the opponent has been gone long enough (Lichess).
    ClaimDraw,
}

impl SeatAction {
    pub const ALL: [Self; 11] = [
        Self::Move,
        Self::OfferDraw,
        Self::AcceptDraw,
        Self::DeclineDraw,
        Self::Takeback,
        Self::AcceptTakeback,
        Self::DeclineTakeback,
        Self::Resign,
        Self::Abort,
        Self::ClaimVictory,
        Self::ClaimDraw,
    ];

    /// The name in routes and the CLI: `offer_draw`.
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Move => "move",
            Self::OfferDraw => "offer_draw",
            Self::AcceptDraw => "accept_draw",
            Self::DeclineDraw => "decline_draw",
            Self::Takeback => "takeback",
            Self::AcceptTakeback => "accept_takeback",
            Self::DeclineTakeback => "decline_takeback",
            Self::Resign => "resign",
            Self::Abort => "abort",
            Self::ClaimVictory => "claim_victory",
            Self::ClaimDraw => "claim_draw",
        }
    }

    pub fn parse(text: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|a| a.as_str() == text.trim())
    }
}

/// What a seat is asked to move in: everything a seat controller needs to choose its move, borrowed from the game.
#[derive(Clone, Copy, Debug)]
pub struct Turn<'a> {
    pub game: GameId,
    pub side: Side,
    /// The ply the move will be, from 1.
    pub ply: u32,
    /// The start position (none for the standard start) and the moves since, in UCI, for `position ... moves ...`.
    pub start_fen: Option<&'a str>,
    pub moves: &'a [String],
    /// The position now.
    pub fen: &'a str,
    pub time: TimeControl,
    /// Both clocks now, in a game with a clock.
    pub clocks: Option<SidePair<u64>>,
}

/// A seat's answer to a [`Turn`].
#[derive(Clone, Debug, PartialEq)]
pub struct SeatMove {
    pub uci: String,
    /// The engine's own score of the position it moved from, from White's point of view.
    pub eval: Option<Score>,
    pub depth: Option<u32>,
    pub nodes: Option<u64>,
}

impl SeatMove {
    pub fn bare(uci: impl Into<String>) -> Self {
        Self { uci: uci.into(), eval: None, depth: None, nodes: None }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sources_are_tagged() {
        let s =
            Source::Lichess { connection: ConnectionKey::You, id: "q7ZvsdUF".into(), rated: true, speed: Speed::Blitz };
        let json = serde_json::to_value(&s).unwrap();
        assert_eq!(json["type"], "lichess");
        assert_eq!(json["connection"], "you");
        assert_eq!(s.url("https://lichess.org/"), Some("https://lichess.org/q7ZvsdUF".into()));
        assert_eq!(serde_json::to_value(Source::Local).unwrap(), serde_json::json!({ "type": "local" }));
    }

    #[test]
    fn outcomes_read_as_pgn() {
        assert_eq!(Outcome::parse_pgn("0-1"), Some(Outcome::BlackWins));
        assert_eq!(Outcome::parse_pgn("*"), None);
        assert_eq!(GameEnd::aborted().pgn(), "*");
        assert_eq!(GameEnd::lost_by(Side::White, Ending::Timeout).outcome, Some(Outcome::BlackWins));
        assert_eq!(Outcome::Draw.points(Side::Black), 0.5);
        assert_eq!(serde_json::to_value(SeatAction::ClaimVictory).unwrap(), "claim_victory");
        assert_eq!(SeatAction::parse("offer_draw"), Some(SeatAction::OfferDraw));
    }

    #[test]
    fn opening_families_drop_the_code_and_the_variation() {
        assert_eq!(opening_family("C20 King's Pawn Game: Busch-Gass Gambit"), "King's Pawn Game");
        assert_eq!(opening_family("B01 Scandinavian Defense"), "Scandinavian Defense");
        assert_eq!(opening_family("Englund Gambit"), "Englund Gambit");
    }
}
