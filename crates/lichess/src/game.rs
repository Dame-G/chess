//! What a game stream sends, for the Bot API (`/api/bot/game/stream/{id}`) and the Board API
//! (`/api/board/game/stream/{id}`) alike: the whole game first (`gameFull`), then a `gameState` after every move,
//! offer and ending, chat lines, and word when the opponent leaves or comes back. Lichess closes the stream when the
//! game is over.

use serde::{Deserialize, Serialize};

use crate::{GameStatus, Side, Variant};

/// One line of a game stream. Types Lichess adds later read as `Other`.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "camelCase")]
pub enum GameEvent {
    GameFull(Box<GameFull>),
    GameState(GameState),
    ChatLine(ChatLine),
    OpponentGone(OpponentGone),
    #[serde(other)]
    Other,
}

/// The game as it stands when the stream opens.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GameFull {
    pub id: String,
    #[serde(default)]
    pub rated: bool,
    #[serde(default)]
    pub speed: Option<String>,
    #[serde(default)]
    pub variant: Variant,
    /// None for correspondence and unlimited games.
    #[serde(default)]
    pub clock: Option<GameClock>,
    #[serde(default)]
    pub days_per_turn: Option<u32>,
    #[serde(default)]
    pub white: GamePlayer,
    #[serde(default)]
    pub black: GamePlayer,
    /// `startpos` or a FEN.
    #[serde(default)]
    pub initial_fen: Option<String>,
    #[serde(default)]
    pub state: GameState,
    /// ms since 1970.
    #[serde(default)]
    pub created_at: Option<i64>,
}

impl GameFull {
    /// The start position's FEN, or `None` for the standard start.
    pub fn start_fen(&self) -> Option<&str> {
        self.initial_fen.as_deref().filter(|f| !f.is_empty() && *f != "startpos")
    }

    /// The side `user_id` plays, if either.
    pub fn side_of(&self, user_id: &str) -> Option<Side> {
        let is = |p: &GamePlayer| p.id.as_deref().is_some_and(|id| id.eq_ignore_ascii_case(user_id));
        if is(&self.white) {
            Some(Side::White)
        } else if is(&self.black) {
            Some(Side::Black)
        } else {
            None
        }
    }

    pub fn player(&self, side: Side) -> &GamePlayer {
        match side {
            Side::White => &self.white,
            Side::Black => &self.black,
        }
    }
}

/// A game's clock, in milliseconds.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct GameClock {
    pub initial: u64,
    pub increment: u64,
}

/// A seat in a game stream: a Lichess user, or Lichess's AI (`ai_level`, no id).
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GamePlayer {
    #[serde(default)]
    pub id: Option<String>,
    #[serde(default)]
    pub name: Option<String>,
    #[serde(default)]
    pub title: Option<String>,
    #[serde(default)]
    pub rating: Option<u32>,
    #[serde(default)]
    pub provisional: bool,
    #[serde(default)]
    pub ai_level: Option<u8>,
}

/// The game after a change: every move so far, the clocks, the status, and the offers standing.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct GameState {
    /// Every move so far in UCI notation, separated by spaces.
    #[serde(default)]
    pub moves: String,
    /// White's clock, ms.
    #[serde(default)]
    pub wtime: u64,
    #[serde(default)]
    pub btime: u64,
    /// White's increment, ms.
    #[serde(default)]
    pub winc: u64,
    #[serde(default)]
    pub binc: u64,
    #[serde(default)]
    pub status: GameStatus,
    #[serde(default)]
    pub winner: Option<Side>,
    /// White offers a draw.
    #[serde(default)]
    pub wdraw: bool,
    #[serde(default)]
    pub bdraw: bool,
    /// White proposes a takeback.
    #[serde(default)]
    pub wtakeback: bool,
    #[serde(default)]
    pub btakeback: bool,
}

impl GameState {
    /// The moves, one by one.
    pub fn moves(&self) -> impl Iterator<Item = &str> {
        self.moves.split_whitespace()
    }

    pub fn plies(&self) -> usize {
        self.moves().count()
    }

    /// The side offering a draw, if one is.
    pub fn draw_offer(&self) -> Option<Side> {
        if self.wdraw {
            Some(Side::White)
        } else if self.bdraw {
            Some(Side::Black)
        } else {
            None
        }
    }

    /// The side proposing a takeback, if one is.
    pub fn takeback_offer(&self) -> Option<Side> {
        if self.wtakeback {
            Some(Side::White)
        } else if self.btakeback {
            Some(Side::Black)
        } else {
            None
        }
    }
}

/// A chat message.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct ChatLine {
    #[serde(default)]
    pub username: String,
    #[serde(default)]
    pub text: String,
    /// `player` or `spectator`.
    #[serde(default)]
    pub room: String,
}

/// The opponent left (`gone`) or came back. Once `claim_win_in_seconds` has passed, the win (or a draw) may be
/// claimed.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct OpponentGone {
    #[serde(default)]
    pub gone: bool,
    #[serde(default)]
    pub claim_win_in_seconds: Option<u64>,
}

/// A chat room of a game.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ChatRoom {
    Player,
    Spectator,
}

impl ChatRoom {
    pub(crate) fn key(self) -> &'static str {
        match self {
            Self::Player => "player",
            Self::Spectator => "spectator",
        }
    }
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;

    #[test]
    fn reads_a_game_stream() {
        let full: GameEvent = serde_json::from_value(json!({
            "type": "gameFull", "id": "g1", "rated": true, "speed": "blitz", "variant": { "key": "standard" },
            "clock": { "initial": 180_000, "increment": 2000 },
            "white": { "id": "recklessbot", "name": "RecklessBot", "title": "BOT", "rating": 2000 },
            "black": { "aiLevel": 3 },
            "initialFen": "startpos",
            "state": { "type": "gameState", "moves": "e2e4 e7e5", "wtime": 179_000, "btime": 178_000,
                "winc": 2000, "binc": 2000, "status": "started" }
        }))
        .unwrap();
        let GameEvent::GameFull(full) = full else { panic!() };
        assert_eq!(full.start_fen(), None);
        assert_eq!(full.side_of("RecklessBot"), Some(Side::White));
        assert_eq!(full.black.ai_level, Some(3));
        assert_eq!(full.state.plies(), 2);
        let state: GameEvent = serde_json::from_value(json!({ "type": "gameState", "moves": "e2e4", "wtime": 1,
            "btime": 2, "winc": 0, "binc": 0, "status": "outoftime", "winner": "black", "bdraw": true }))
        .unwrap();
        let GameEvent::GameState(state) = state else { panic!() };
        assert!(state.status.is_over());
        assert_eq!(state.winner, Some(Side::Black));
        assert_eq!(state.draw_offer(), Some(Side::Black));
        assert_eq!(state.takeback_offer(), None);
        let gone: GameEvent =
            serde_json::from_value(json!({ "type": "opponentGone", "gone": true, "claimWinInSeconds": 8 })).unwrap();
        assert_eq!(gone, GameEvent::OpponentGone(OpponentGone { gone: true, claim_win_in_seconds: Some(8) }));
        let odd: GameEvent = serde_json::from_value(json!({ "type": "gameState", "status": "brandNew" })).unwrap();
        let GameEvent::GameState(odd) = odd else { panic!() };
        assert_eq!(odd.status, GameStatus::Other);
    }
}
