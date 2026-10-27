//! The account's event stream (`GET /api/stream/event`): games starting and finishing, challenges arriving, withdrawn
//! and declined. It starts with every open challenge and every game in progress, so a client that reconnects learns
//! what it missed. Reconnecting with backoff is the caller's loop, with [`crate::Backoff`].

use reqwest::Method;
use serde::{Deserialize, Serialize};

use crate::{Compat, Error, GameStatus, Lichess, Ndjson, Side, Variant, client::Body};

/// One event. Types Lichess adds later read as `Other`.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "camelCase")]
pub enum Event {
    GameStart {
        game: GameInfo,
    },
    GameFinish {
        game: GameInfo,
    },
    /// A challenge to us, or one of ours (Lichess sends both).
    Challenge {
        challenge: Challenge,
        #[serde(default)]
        compat: Option<Compat>,
    },
    /// Withdrawn by whoever made it.
    ChallengeCanceled {
        challenge: Challenge,
    },
    /// Declined by the one challenged; `challenge.decline_reason_key` says why.
    ChallengeDeclined {
        challenge: Challenge,
    },
    #[serde(other)]
    Other,
}

/// A game as the event stream and `GET /api/account/playing` describe it, from our side.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GameInfo {
    /// The game id. Events send it as `gameId` and as `id`; use [`GameInfo::id`].
    #[serde(default)]
    pub game_id: Option<String>,
    #[serde(default)]
    pub id: Option<String>,
    /// The id with our player's suffix.
    #[serde(default)]
    pub full_id: Option<String>,
    /// Our side.
    #[serde(default)]
    pub color: Option<Side>,
    #[serde(default)]
    pub fen: Option<String>,
    #[serde(default)]
    pub has_moved: bool,
    #[serde(default)]
    pub is_my_turn: bool,
    #[serde(default)]
    pub last_move: Option<String>,
    #[serde(default)]
    pub opponent: Option<Opponent>,
    /// The perf key (`blitz`, `correspondence`, …).
    #[serde(default)]
    pub perf: Option<String>,
    #[serde(default)]
    pub rated: bool,
    #[serde(default)]
    pub seconds_left: Option<u64>,
    /// `friend`, `lobby`, `ai`, `pool`, …
    #[serde(default)]
    pub source: Option<String>,
    #[serde(default)]
    pub status: Option<StatusInfo>,
    #[serde(default)]
    pub speed: Option<String>,
    #[serde(default)]
    pub variant: Option<Variant>,
    #[serde(default)]
    pub compat: Option<Compat>,
    /// `gameFinish`: who won, if anyone.
    #[serde(default)]
    pub winner: Option<Side>,
    #[serde(default)]
    pub rating_diff: Option<i32>,
}

impl GameInfo {
    /// The game id, whichever field carried it.
    pub fn id(&self) -> &str {
        self.game_id.as_deref().or(self.id.as_deref()).unwrap_or_default()
    }
}

/// The other player of a game, from our side.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct Opponent {
    #[serde(default)]
    pub id: Option<String>,
    #[serde(default)]
    pub username: Option<String>,
    #[serde(default)]
    pub rating: Option<u32>,
    /// Lichess's AI, at this level.
    #[serde(default)]
    pub ai: Option<u8>,
}

/// A status as events send it: `{ "id": 30, "name": "mate" }`.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct StatusInfo {
    #[serde(default)]
    pub id: u32,
    #[serde(default)]
    pub name: GameStatus,
}

/// A challenge, as events, `GET /api/challenge` and the challenge routes send it.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Challenge {
    pub id: String,
    #[serde(default)]
    pub url: Option<String>,
    /// `created`, `offline`, `canceled`, `declined`, `accepted`.
    #[serde(default)]
    pub status: Option<String>,
    #[serde(default)]
    pub challenger: Option<ChallengeUser>,
    #[serde(default)]
    pub dest_user: Option<ChallengeUser>,
    #[serde(default)]
    pub variant: Variant,
    #[serde(default)]
    pub rated: bool,
    #[serde(default)]
    pub speed: String,
    #[serde(default)]
    pub time_control: ChallengeTimeControl,
    /// The challenger's choice: `white`, `black` or `random`.
    #[serde(default)]
    pub color: Option<String>,
    /// The challenger's side once decided.
    #[serde(default)]
    pub final_color: Option<Side>,
    #[serde(default)]
    pub perf: Option<PerfName>,
    #[serde(default)]
    pub initial_fen: Option<String>,
    /// `in` or `out`, in `GET /api/challenge`.
    #[serde(default)]
    pub direction: Option<String>,
    /// For people, when declined.
    #[serde(default)]
    pub decline_reason: Option<String>,
    /// Lichess's key, when declined: `generic`, `later`, `tooFast`, …
    #[serde(default)]
    pub decline_reason_key: Option<String>,
}

impl Challenge {
    /// Whether `user_id` made it.
    pub fn is_from(&self, user_id: &str) -> bool {
        self.challenger.as_ref().is_some_and(|u| u.id.eq_ignore_ascii_case(user_id))
    }
}

/// Someone in a challenge.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct ChallengeUser {
    #[serde(default)]
    pub id: String,
    #[serde(default)]
    pub name: String,
    #[serde(default)]
    pub title: Option<String>,
    #[serde(default)]
    pub rating: Option<u32>,
    #[serde(default)]
    pub provisional: bool,
    #[serde(default)]
    pub online: Option<bool>,
}

/// A challenge's clock.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ChallengeTimeControl {
    /// `clock`, `correspondence` or `unlimited`.
    #[serde(rename = "type", default)]
    pub kind: String,
    /// Seconds.
    #[serde(default)]
    pub limit: Option<u32>,
    /// Seconds.
    #[serde(default)]
    pub increment: Option<u32>,
    #[serde(default)]
    pub days_per_turn: Option<u32>,
    /// As people write it: `3+2`.
    #[serde(default)]
    pub show: Option<String>,
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct PerfName {
    #[serde(default)]
    pub name: Option<String>,
}

impl Lichess {
    /// Opens the account's event stream.
    pub async fn events(&self) -> Result<Ndjson<Event>, Error> {
        self.need_token()?;
        self.stream(Method::GET, "/api/stream/event", Body::None).await
    }
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;
    use crate::testing::{GameSetup, MockAccount, MockLichess, Seat};

    #[test]
    fn reads_every_kind_of_event_and_skips_new_ones() {
        let start: Event = serde_json::from_value(json!({ "type": "gameStart", "game": {
            "gameId": "g1", "id": "g1", "fullId": "g1abcd", "color": "white", "fen": "startpos", "hasMoved": false,
            "isMyTurn": true, "lastMove": "", "opponent": { "id": null, "username": "A.I. level 3", "ai": 3 },
            "perf": "blitz", "rated": false, "secondsLeft": 300, "source": "ai", "status": { "id": 20, "name": "started" },
            "speed": "blitz", "variant": { "key": "standard", "name": "Standard" }, "compat": { "bot": false, "board": true }
        }}))
        .unwrap();
        let Event::GameStart { game } = start else { panic!() };
        assert_eq!(game.id(), "g1");
        assert_eq!(game.opponent.unwrap().ai, Some(3));
        assert_eq!(game.status.unwrap().name, GameStatus::Started);
        assert!(game.compat.unwrap().board);
        let declined: Event = serde_json::from_value(json!({ "type": "challengeDeclined", "challenge": {
            "id": "c1", "challenger": { "id": "dame", "name": "Dame" }, "destUser": { "id": "alpha", "name": "Alpha", "title": "BOT" },
            "variant": { "key": "standard" }, "rated": true, "speed": "blitz",
            "timeControl": { "type": "clock", "limit": 180, "increment": 2, "show": "3+2" },
            "declineReason": "This time control is too fast for me.", "declineReasonKey": "tooFast"
        }}))
        .unwrap();
        let Event::ChallengeDeclined { challenge } = declined else { panic!() };
        assert!(challenge.is_from("Dame"));
        assert_eq!(challenge.decline_reason_key.as_deref(), Some("tooFast"));
        assert_eq!(challenge.time_control.limit, Some(180));
        let new: Event = serde_json::from_value(json!({ "type": "somethingNew", "x": 1 })).unwrap();
        assert_eq!(new, Event::Other);
    }

    #[tokio::test]
    async fn streams_games_and_challenges_as_they_happen() {
        let mock = MockLichess::start().await;
        mock.add_account("tok", MockAccount::user("Dame"));
        let mut events = mock.client(Some("tok")).events().await.unwrap();
        let game = mock.start_game(GameSetup::new(Seat::account("dame"), Seat::user("Alice", 1700)));
        let Some(Event::GameStart { game: info }) = events.next().await.unwrap() else { panic!() };
        assert_eq!(info.id(), game);
        assert_eq!(info.color, Some(Side::White));
        assert_eq!(info.opponent.unwrap().username.as_deref(), Some("Alice"));
        mock.end_game(&game, GameStatus::Resign, Some(Side::Black));
        let Some(Event::GameFinish { game: info }) = events.next().await.unwrap() else { panic!() };
        assert_eq!(info.winner, Some(Side::Black));
        assert_eq!(info.status.unwrap().name, GameStatus::Resign);
        // Anything a test pushes arrives as it is.
        mock.push_event("dame", json!({ "type": "challengeCanceled", "challenge": { "id": "zz" } }));
        let Some(Event::ChallengeCanceled { challenge }) = events.next().await.unwrap() else { panic!() };
        assert_eq!(challenge.id, "zz");
    }

    #[tokio::test]
    async fn a_new_connection_learns_the_games_in_progress() {
        let mock = MockLichess::start().await;
        mock.add_account("tok", MockAccount::user("Dame"));
        let game = mock.start_game(GameSetup::new(Seat::user("Alice", 1700), Seat::account("dame")));
        let mut events = mock.client(Some("tok")).events().await.unwrap();
        let Some(Event::GameStart { game: info }) = events.next().await.unwrap() else { panic!() };
        assert_eq!((info.id(), info.color), (game.as_str(), Some(Side::Black)));
    }
}
