//! A user's games (`GET /api/games/user/{username}`), as NDJSON with moves, clocks, Lichess's evaluations and the
//! opening, newest first. A long export (thousands of games with evals) can outlive its connection, so a stream that
//! breaks is reopened below the oldest game already read, a few times in a row, before the export fails.

use reqwest::Method;
use serde::{Deserialize, Serialize};

use crate::{
    Error, GameStatus, Lichess, Ndjson, Side,
    client::{Body, seg, with_query},
};

/// How many times in a row a broken export is reopened before it fails.
pub const MAX_BREAKS: u32 = 5;

/// Which games to export.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct ExportQuery {
    /// Games started at or after this time, ms since 1970.
    pub since: Option<i64>,
    /// Games started at or before this time, ms since 1970.
    pub until: Option<i64>,
    /// At most this many games.
    pub max: Option<u32>,
    /// Only these perfs (`bullet`, `blitz`, `rapid`, `classical`, `correspondence`, …). All when empty.
    pub perf_type: Vec<String>,
    /// Only rated (true) or casual (false) games.
    pub rated: Option<bool>,
}

impl ExportQuery {
    fn path(&self, username: &str, until: Option<i64>, read: u32) -> String {
        let mut params = vec![
            ("finished", "true".to_owned()),
            ("moves", "true".to_owned()),
            ("clocks", "true".to_owned()),
            ("evals", "true".to_owned()),
            ("opening", "true".to_owned()),
            ("sort", "dateDesc".to_owned()),
        ];
        if let Some(s) = self.since {
            params.push(("since", s.to_string()));
        }
        if let Some(u) = until.or(self.until) {
            params.push(("until", u.to_string()));
        }
        if let Some(max) = self.max {
            params.push(("max", max.saturating_sub(read).max(1).to_string()));
        }
        if !self.perf_type.is_empty() {
            params.push(("perfType", self.perf_type.join(",")));
        }
        if let Some(r) = self.rated {
            params.push(("rated", r.to_string()));
        }
        with_query(&format!("/api/games/user/{}", seg(username)), &params)
    }
}

/// One exported game.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ExportedGame {
    pub id: String,
    #[serde(default)]
    pub rated: bool,
    /// `standard`, `chess960`, `fromPosition`, …
    #[serde(default)]
    pub variant: String,
    #[serde(default)]
    pub speed: String,
    #[serde(default)]
    pub perf: Option<String>,
    /// ms since 1970.
    #[serde(default)]
    pub created_at: i64,
    #[serde(default)]
    pub last_move_at: Option<i64>,
    #[serde(default)]
    pub status: GameStatus,
    #[serde(default)]
    pub winner: Option<Side>,
    #[serde(default)]
    pub players: ExportPlayers,
    #[serde(default)]
    pub opening: Option<ExportOpening>,
    /// The moves in SAN, separated by spaces.
    #[serde(default)]
    pub moves: String,
    /// The mover's clock after each move, in centiseconds; the first is White's after its first move.
    #[serde(default)]
    pub clocks: Vec<u64>,
    /// Lichess's evaluation after each move, where the game was analysed.
    #[serde(default)]
    pub analysis: Vec<Analysis>,
    #[serde(default)]
    pub clock: Option<ExportClock>,
    #[serde(default)]
    pub days_per_turn: Option<u32>,
    /// The start position, when not the standard one.
    #[serde(default)]
    pub initial_fen: Option<String>,
    #[serde(default)]
    pub source: Option<String>,
}

impl ExportedGame {
    /// The side `user_id` played, if either.
    pub fn side_of(&self, user_id: &str) -> Option<Side> {
        let is = |p: &ExportPlayer| p.user.as_ref().is_some_and(|u| u.id.eq_ignore_ascii_case(user_id));
        if is(&self.players.white) {
            Some(Side::White)
        } else if is(&self.players.black) {
            Some(Side::Black)
        } else {
            None
        }
    }
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct ExportPlayers {
    #[serde(default)]
    pub white: ExportPlayer,
    #[serde(default)]
    pub black: ExportPlayer,
}

/// A player of an exported game: a user (or anonymous), or Lichess's AI.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ExportPlayer {
    #[serde(default)]
    pub user: Option<LightUser>,
    #[serde(default)]
    pub rating: Option<u32>,
    #[serde(default)]
    pub rating_diff: Option<i32>,
    #[serde(default)]
    pub provisional: bool,
    #[serde(default)]
    pub ai_level: Option<u8>,
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct LightUser {
    #[serde(default)]
    pub id: String,
    #[serde(default)]
    pub name: String,
    #[serde(default)]
    pub title: Option<String>,
}

/// The opening, as Lichess names it, and the ply where it ends.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct ExportOpening {
    #[serde(default)]
    pub eco: Option<String>,
    #[serde(default)]
    pub name: Option<String>,
    #[serde(default)]
    pub ply: Option<u32>,
}

/// An evaluation after a move, from White's side: centipawns or a mate in N.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct Analysis {
    #[serde(default)]
    pub eval: Option<i32>,
    #[serde(default)]
    pub mate: Option<i32>,
    /// Lichess's better move (UCI), where it judged the move.
    #[serde(default)]
    pub best: Option<String>,
    #[serde(default)]
    pub variation: Option<String>,
    #[serde(default)]
    pub judgment: Option<Judgment>,
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct Judgment {
    /// `Inaccuracy`, `Mistake` or `Blunder`.
    #[serde(default)]
    pub name: String,
    #[serde(default)]
    pub comment: Option<String>,
}

/// A game's clock, in seconds.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ExportClock {
    pub initial: u32,
    pub increment: u32,
    #[serde(default)]
    pub total_time: Option<u32>,
}

/// A break in an export, as it is reopened: for the caller's log.
#[derive(Clone, Debug)]
pub struct ExportBreak {
    /// Breaks in a row, this one included.
    pub breaks: u32,
    /// Games read so far.
    pub read: u32,
    pub error: Error,
}

/// What is told of each break.
type OnBreak = Box<dyn FnMut(&ExportBreak) + Send>;

/// An export under way. Call [`GameExport::next`] until it answers `None`.
pub struct GameExport {
    client: Lichess,
    username: String,
    query: ExportQuery,
    stream: Option<Ndjson<ExportedGame>>,
    read: u32,
    oldest: Option<i64>,
    breaks: u32,
    on_break: Option<OnBreak>,
}

impl Lichess {
    /// Starts exporting `username`'s finished games, newest first. Anonymous clients may export too (more slowly).
    pub fn export_games(&self, username: &str, query: ExportQuery) -> GameExport {
        GameExport {
            client: self.clone(),
            username: username.to_owned(),
            query,
            stream: None,
            read: 0,
            oldest: None,
            breaks: 0,
            on_break: None,
        }
    }
}

impl GameExport {
    /// Tells `f` of each break, as the export is about to be reopened.
    pub fn on_break(mut self, f: impl FnMut(&ExportBreak) + Send + 'static) -> Self {
        self.on_break = Some(Box::new(f));
        self
    }

    /// Games read so far.
    pub fn read(&self) -> u32 {
        self.read
    }

    /// The next game, or `None` at the end. `Error::Unexpected` is one game that does not read, and the export goes on;
    /// any other error means it failed (after [`MAX_BREAKS`] breaks in a row, or a refusal such as 404 for an unknown
    /// user).
    pub async fn next(&mut self) -> Result<Option<ExportedGame>, Error> {
        loop {
            if self.query.max.is_some_and(|m| self.read >= m) {
                self.stream = None;
                return Ok(None);
            }
            let outcome = match self.stream.as_mut() {
                Some(stream) => stream.next().await,
                None => {
                    // Below the oldest game read, so a reopened export does not send a game twice.
                    let path = self.query.path(&self.username, self.oldest.map(|o| o - 1), self.read);
                    match self.client.stream(Method::GET, &path, Body::None).await {
                        Ok(stream) => {
                            self.stream = Some(stream);
                            continue;
                        }
                        Err(e) => Err(e),
                    }
                }
            };
            match outcome {
                Ok(Some(game)) => {
                    self.read += 1;
                    self.breaks = 0;
                    self.oldest = Some(self.oldest.map_or(game.created_at, |o| o.min(game.created_at)));
                    return Ok(Some(game));
                }
                Ok(None) => {
                    self.stream = None;
                    self.query.max = Some(self.read);
                    return Ok(None);
                }
                Err(e @ Error::Unexpected(_)) => {
                    self.read += 1;
                    return Err(e);
                }
                Err(e @ (Error::Refused { .. } | Error::NoToken)) => return Err(e),
                Err(e) if self.breaks < MAX_BREAKS => {
                    self.stream = None;
                    self.breaks += 1;
                    if let Some(f) = self.on_break.as_mut() {
                        f(&ExportBreak { breaks: self.breaks, read: self.read, error: e.clone() });
                    }
                    // After a 429 the client itself waits its minute before the next request.
                    if !matches!(e, Error::RateLimited { .. }) {
                        tokio::time::sleep(self.client.config().timing.import_break * self.breaks).await;
                    }
                }
                Err(e) => return Err(e),
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use std::sync::{Arc, Mutex};

    use super::*;
    use crate::testing::{MockAccount, MockLichess, exported_game};

    async fn all(export: &mut GameExport) -> Vec<ExportedGame> {
        let mut games = Vec::new();
        while let Some(g) = export.next().await.unwrap() {
            games.push(g);
        }
        games
    }

    #[tokio::test]
    async fn reads_every_field_of_an_export() {
        let mock = MockLichess::start().await;
        mock.add_account("me", MockAccount::user("Dame"));
        mock.set_games_export("dame", vec![exported_game("imp00001", 1_700_000_000_000, "dame", "bob")]);
        let mut export = mock.client(Some("me")).export_games("Dame", ExportQuery::default());
        let games = all(&mut export).await;
        assert_eq!(games.len(), 1);
        let g = &games[0];
        assert_eq!((g.id.as_str(), g.speed.as_str(), g.rated), ("imp00001", "blitz", true));
        assert_eq!((g.status, g.winner), (GameStatus::Mate, Some(Side::White)));
        assert_eq!(g.side_of("dame"), Some(Side::White));
        assert_eq!(g.players.black.rating, Some(1790));
        assert_eq!(g.opening.as_ref().unwrap().eco.as_deref(), Some("C20"));
        assert_eq!(g.opening.as_ref().unwrap().ply, Some(2));
        assert_eq!(g.moves.split_whitespace().count(), 7);
        assert_eq!(g.clocks.len(), 7);
        assert_eq!(g.analysis[5].mate, Some(1));
        assert_eq!(g.clock.unwrap().initial, 180);
        let q = &mock.requests_to("/api/games/user/Dame")[0].query;
        for flag in ["moves", "clocks", "evals", "opening", "finished"] {
            assert_eq!(q[flag], "true", "{flag}");
        }
        assert_eq!(q["sort"], "dateDesc");
    }

    #[tokio::test]
    async fn filters_by_time_count_perf_and_rated() {
        let mock = MockLichess::start().await;
        let games = (0..5).map(|i| exported_game(&format!("g{i}"), 1_000 + i * 100, "dame", "bob")).collect();
        mock.set_games_export("dame", games);
        let query = ExportQuery {
            since: Some(1_100),
            max: Some(2),
            perf_type: vec!["blitz".into(), "rapid".into()],
            rated: Some(true),
            ..ExportQuery::default()
        };
        let mut export = mock.client(None).export_games("dame", query);
        let ids: Vec<String> = all(&mut export).await.into_iter().map(|g| g.id).collect();
        assert_eq!(ids, ["g4", "g3"]);
        let q = &mock.requests_to("/api/games/user/dame")[0].query;
        assert_eq!((q["since"].as_str(), q["max"].as_str()), ("1100", "2"));
        assert_eq!((q["perfType"].as_str(), q["rated"].as_str()), ("blitz,rapid", "true"));
        // Anonymous: no token at all.
        assert_eq!(mock.requests_to("/api/games/user/dame")[0].token, None);
    }

    #[tokio::test]
    async fn a_broken_export_resumes_below_the_oldest_game_read() {
        let mock = MockLichess::start().await;
        let games = (0..6).map(|i| exported_game(&format!("g{i}"), 1_000 + i * 100, "dame", "bob")).collect();
        mock.set_games_export("dame", games);
        mock.break_export_after(2);
        let breaks = Arc::new(Mutex::new(Vec::new()));
        let seen = breaks.clone();
        let mut export = mock
            .client(None)
            .export_games("dame", ExportQuery { max: Some(5), ..ExportQuery::default() })
            .on_break(move |b| seen.lock().unwrap().push((b.breaks, b.read)));
        let ids: Vec<String> = all(&mut export).await.into_iter().map(|g| g.id).collect();
        assert_eq!(ids, ["g5", "g4", "g3", "g2", "g1"]);
        assert_eq!(*breaks.lock().unwrap(), [(1, 2)]);
        let asked = mock.requests_to("/api/games/user/dame");
        assert_eq!(asked.len(), 2);
        // Below g4 (created at 1400), and three more to make five.
        assert_eq!((asked[1].query["until"].as_str(), asked[1].query["max"].as_str()), ("1399", "3"));
    }

    #[tokio::test]
    async fn gives_up_after_five_breaks_in_a_row_and_fails_on_an_unknown_user() {
        let mock = MockLichess::start().await;
        mock.set_games_export("dame", vec![exported_game("g0", 1_000, "dame", "bob")]);
        for _ in 0..=MAX_BREAKS {
            mock.fail_next("/api/games/user/dame", 503, serde_json::json!({}));
        }
        let mut export = mock.client(None).export_games("dame", ExportQuery::default());
        assert!(matches!(export.next().await, Err(Error::Unreachable(_))));
        assert_eq!(mock.requests_to("/api/games/user/dame").len(), MAX_BREAKS as usize + 1);
        let mut unknown = mock.client(None).export_games("nobody", ExportQuery::default());
        assert_eq!(unknown.next().await.unwrap_err().status(), Some(404));
    }

    #[tokio::test]
    async fn a_429_waits_out_the_client_s_pause_and_carries_on() {
        let mock = MockLichess::start().await;
        mock.set_games_export("dame", vec![exported_game("g0", 1_000, "dame", "bob")]);
        mock.rate_limit_next("/api/games/user/dame");
        let client = mock.client(None);
        let mut export = client.export_games("dame", ExportQuery::default());
        let started = std::time::Instant::now();
        assert_eq!(all(&mut export).await.len(), 1);
        assert!(started.elapsed() >= mock.config().timing.rate_limit_pause / 2);
    }
}
