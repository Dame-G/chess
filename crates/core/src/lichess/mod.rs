//! Lichess's business rules and the shapes the app shows of its connections. The HTTP client and its wire types are
//! the `lichess` adapter's; `play` maps them to these and runs the rules.
//!
//! - [`bot`]: which challenges Dameless's BOT account takes, and its settings.
//! - [`seek`]: how the bot looks for games by challenging other bots (time and randomness passed in).
//! - [`quota`]: Lichess's daily limit of 100 games between bots.
//! - [`board`]: what the Board API lets your own account play (rapid+ seeks, blitz+ challenges, never bullet).
//! - [`ending`]: how a Lichess game status reads as a result.

pub mod board;
pub mod bot;
pub mod quota;
pub mod seek;

use serde::{Deserialize, Serialize};
use utoipa::ToSchema;

use crate::{
    chess::Position,
    game::{Ending, GameEnd, Outcome},
    ids::{Color, ConnectionKey, GameId, Side, Timestamp},
    time::{Speed, TimeControl},
};

pub use bot::{BotSettings, Decline, SeekClock};
pub use quota::Quota;
pub use seek::{SeekMode, SeekStatus};

/// One Lichess rating.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
pub struct Rating {
    pub rating: u32,
    pub games: Option<u32>,
    /// Lichess calls a rating provisional until it has settled (`prov`).
    pub provisional: bool,
}

/// Lichess's ratings per speed, when known.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
pub struct Ratings {
    pub bullet: Option<Rating>,
    pub blitz: Option<Rating>,
    pub rapid: Option<Rating>,
    pub classical: Option<Rating>,
}

/// A rating Lichess calls provisional, or none at all, counts as this when picking opponents.
pub const UNRATED: u32 = 1500;

impl Ratings {
    pub fn get(&self, speed: Speed) -> Option<Rating> {
        match speed {
            Speed::Bullet => self.bullet,
            Speed::Blitz => self.blitz,
            Speed::Rapid => self.rapid,
            Speed::Classical => self.classical,
            Speed::UltraBullet | Speed::Correspondence => None,
        }
    }

    /// The rating at `speed` for matching opponents: a provisional or missing one is [`UNRATED`].
    pub fn settled(&self, speed: Speed) -> u32 {
        match self.get(speed) {
            Some(r) if !r.provisional => r.rating,
            _ => UNRATED,
        }
    }
}

/// A Lichess account as a connection knows it.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
pub struct Account {
    /// The lower-case id.
    pub id: String,
    pub username: String,
    pub title: Option<String>,
    pub ratings: Ratings,
}

impl Account {
    pub fn is_bot(&self) -> bool {
        self.title.as_deref() == Some("BOT")
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum ConnectionState {
    #[default]
    Off,
    Connecting,
    Online,
    Error,
}

/// Someone on the other side of a challenge.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
pub struct Opponent {
    pub name: String,
    pub title: Option<String>,
    pub rating: Option<u32>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum Direction {
    /// Someone challenged us.
    Incoming,
    /// We challenged someone.
    Outgoing,
}

/// A challenge to or from one of the connections.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, ToSchema)]
pub struct Challenge {
    /// Lichess's id, which becomes the game's id when accepted.
    pub id: String,
    pub connection: ConnectionKey,
    pub direction: Direction,
    pub opponent: Opponent,
    pub rated: bool,
    pub speed: Speed,
    /// Lichess's variant key: `standard`, `fromPosition`, `chess960`, …
    pub variant: String,
    /// None for unlimited games.
    pub time: Option<TimeControl>,
    /// The colour the connection would play.
    pub color: Color,
    /// Whether this app can play it (the Board API cannot play bullet or variants).
    pub playable: bool,
    pub url: String,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum ChallengeEventKind {
    Created,
    Accepted,
    Declined,
    Canceled,
}

/// The `challenge` event: what became of a challenge, with Lichess's decline key and a few words more.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, ToSchema)]
pub struct ChallengeEvent {
    pub challenge: Challenge,
    pub event: ChallengeEventKind,
    pub reason: Option<String>,
    pub detail: Option<String>,
}

/// A game of yours on Lichess that cannot be played here (bullet, a variant): it still turns analysis off.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
pub struct ElsewhereGame {
    pub id: String,
    pub url: String,
    pub speed: Speed,
}

/// Your seek in the lobby, open until someone takes it.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
pub struct YouSeek {
    pub time: TimeControl,
    pub rated: bool,
    pub color: Color,
    /// `1500-1800`.
    pub rating_range: Option<String>,
}

/// What only your connection has.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, ToSchema)]
pub struct YouStatus {
    pub scopes: Vec<String>,
    pub missing_scopes: Vec<String>,
    pub expires_at: Option<Timestamp>,
    /// Your games on Lichess that cannot be played here.
    pub elsewhere: Vec<ElsewhereGame>,
    pub seek: Option<YouSeek>,
    /// How the last seek went, for people.
    pub seek_note: Option<String>,
}

/// What only the bot's connection has.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, ToSchema)]
pub struct BotStatus {
    /// Online across restarts until stopped.
    pub online: bool,
    pub settings: BotSettings,
    pub seek: SeekStatus,
    /// Games against bots started in the last 24 hours.
    pub bot_games_today: u32,
    /// While at the daily limit: when it lifts.
    pub bot_limit_until: Option<Timestamp>,
}

/// A connection as the app shows it: the `connection.upserted` event and `GET /api/connections`.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, ToSchema)]
pub struct ConnectionStatus {
    pub key: ConnectionKey,
    /// Whether a token is stored.
    pub configured: bool,
    pub account: Option<Account>,
    pub state: ConnectionState,
    pub error: Option<String>,
    /// Games being played here through this connection.
    pub playing: Vec<GameId>,
    pub challenges: Vec<Challenge>,
    pub you: Option<YouStatus>,
    pub bot: Option<BotStatus>,
}

/// How a Lichess game status ends the record, or `None` while it is still being played. `winner` is Lichess's;
/// `pos` tells a drawn game's reason apart (a claimed repetition, fifty moves or bare kings; else an agreement).
///
/// Statuses our [`Ending`] has no word for (`noStart`, `cheat`, `variantEnd`, unknown ones) are `Other`; `noStart`
/// with no winner is an abort, as Lichess shows it.
pub fn ending(status: &str, winner: Option<Side>, pos: Option<&Position>) -> Option<GameEnd> {
    let ending = match status {
        "created" | "started" => return None,
        "aborted" => return Some(GameEnd::aborted()),
        "noStart" if winner.is_none() => return Some(GameEnd::aborted()),
        "mate" => Ending::Checkmate,
        "resign" => Ending::Resignation,
        "stalemate" => Ending::Stalemate,
        "timeout" => Ending::Abandoned,
        "outoftime" => Ending::Timeout,
        // A win on time claimed against a side that could not have mated: a draw.
        "insufficientMaterialClaim" => Ending::InsufficientMaterial,
        // A draw by agreement, or claimed: the position says which rule, if any, was claimed.
        "draw" => match pos.and_then(Position::ending) {
            Some(e) if e.outcome == Some(Outcome::Draw) => e.ending,
            _ => Ending::Agreement,
        },
        _ => Ending::Other,
    };
    Some(GameEnd { outcome: Some(winner.map_or(Outcome::Draw, Outcome::won_by)), ending })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn maps_every_lichess_status() {
        let start = Position::start();
        let end = |status, winner| ending(status, winner, Some(&start));
        assert_eq!(end("started", None), None);
        assert_eq!(end("created", None), None);
        assert_eq!(end("aborted", None), Some(GameEnd::aborted()));
        assert_eq!(end("mate", Some(Side::Black)), Some(GameEnd::new(Outcome::BlackWins, Ending::Checkmate)));
        assert_eq!(end("resign", Some(Side::White)), Some(GameEnd::new(Outcome::WhiteWins, Ending::Resignation)));
        assert_eq!(end("stalemate", None), Some(GameEnd::draw(Ending::Stalemate)));
        assert_eq!(end("timeout", Some(Side::White)).unwrap().ending, Ending::Abandoned);
        // A player who left can also be claimed as a draw.
        assert_eq!(end("timeout", None).unwrap().outcome, Some(Outcome::Draw));
        assert_eq!(end("outoftime", Some(Side::Black)), Some(GameEnd::new(Outcome::BlackWins, Ending::Timeout)));
        // Out of time against bare kings is a draw.
        assert_eq!(end("outoftime", None).unwrap().outcome, Some(Outcome::Draw));
        assert_eq!(end("cheat", Some(Side::White)).unwrap().ending, Ending::Other);
        assert_eq!(end("noStart", Some(Side::Black)).unwrap().ending, Ending::Other);
        assert_eq!(end("noStart", None), Some(GameEnd::aborted()));
        assert_eq!(end("variantEnd", Some(Side::White)).unwrap().ending, Ending::Other);
        assert_eq!(end("unknownFinish", None).unwrap().ending, Ending::Other);
        assert_eq!(end("insufficientMaterialClaim", None), Some(GameEnd::draw(Ending::InsufficientMaterial)));
        assert_eq!(end("draw", None), Some(GameEnd::draw(Ending::Agreement)));
    }

    #[test]
    fn a_claimed_draw_names_the_rule() {
        let bare = Position::from_fen("8/8/4k3/8/8/3K4/8/8 w - - 0 1").unwrap();
        assert_eq!(ending("draw", None, Some(&bare)).unwrap().ending, Ending::InsufficientMaterial);
        let fifty = Position::from_fen("4k3/8/8/8/8/8/8/R3K3 w - - 100 80").unwrap();
        assert_eq!(ending("draw", None, Some(&fifty)).unwrap().ending, Ending::FiftyMoves);
    }

    #[test]
    fn ratings_count_provisional_as_unrated() {
        let r = Ratings {
            blitz: Some(Rating { rating: 1873, games: None, provisional: false }),
            bullet: Some(Rating { rating: 2100, games: None, provisional: true }),
            ..Ratings::default()
        };
        assert_eq!(r.settled(Speed::Blitz), 1873);
        assert_eq!(r.settled(Speed::Bullet), 1500);
        assert_eq!(r.settled(Speed::Rapid), 1500);
    }
}
