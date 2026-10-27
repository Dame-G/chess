//! Dameless on Lichess, against the mock: challenges taken and declined by the settings, games played with the fake
//! engine (claiming when the opponent leaves, resigning when the engine dies), seeking other bots, and the daily limit
//! on games between bots.

mod common;

use std::time::Duration;

use common::*;
use dameless_core::{
    ConnectionKey, GameId, PlayerKey, Speed,
    events::EventKind,
    game::{Ending, Game, Outcome},
    lichess::{BotSettings, ConnectionState, SeekMode},
};
use dameless_lichess::{
    GameStatus as LichessStatus, Side as LichessSide,
    challenge::{ChallengeRequest, DeclineReason},
    testing::{ChallengeAnswer, MockAccount, MockLichess, Seat},
};
use dameless_play::{Binaries, Connections, FixedBinaries};
use serde_json::json;

struct Lichess {
    s: Setup,
    mock: MockLichess,
    connections: Connections,
}

async fn lichess(binaries: std::sync::Arc<dyn Binaries>) -> Lichess {
    let s = setup(binaries);
    let mock = MockLichess::start().await;
    mock.add_account("bot-tok", MockAccount::bot("DamelessBot").rating("blitz", 2000));
    let connections = Connections::new(s.hub.clone(), s.games.clone(), mock.config());
    connections.start().await.unwrap();
    connections.set_token(ConnectionKey::Bot, "bot-tok").await.unwrap();
    let settings = BotSettings {
        version: Some("champion".into()),
        speeds: vec![Speed::Blitz, Speed::Rapid],
        seek_clocks: vec!["3+2".into()],
        ..BotSettings::default()
    };
    connections.bot().set_settings(settings).await.unwrap();
    connections.bot().set_online(true).await.unwrap();
    until("the bot online", || connections.status(ConnectionKey::Bot).state == ConnectionState::Online).await;
    Lichess { s, mock, connections }
}

impl Lichess {
    async fn game_of(&self, lichess: &str) -> GameId {
        let found = || self.s.games.live().into_iter().find(|g| g.source.lichess_id() == Some(lichess)).map(|g| g.id);
        until("the game to be played here", || found().is_some()).await;
        found().unwrap()
    }

    async fn game(&self, id: GameId) -> Game {
        self.s.games.game(id).await.unwrap()
    }

    fn declined(&self, challenge: &str) -> Option<String> {
        let reqs = self.mock.requests_to(&format!("/api/challenge/{challenge}/decline"));
        reqs.first().map(|r| r.form["reason"].clone())
    }
}

#[tokio::test]
async fn the_bot_takes_challenges_by_its_rules_and_plays_them() {
    let l = lichess(binaries()).await;
    let mut events = subscribe(&l.s.hub);

    // Bullet is not among its speeds; a variant never is.
    let bullet = l.mock.send_challenge(Seat::user("Alice", 1500), "damelessbot", &ChallengeRequest::clock(true, 60, 0));
    until("the decline", || l.declined(&bullet).is_some()).await;
    assert_eq!(l.declined(&bullet).as_deref(), Some("timeControl"));
    let mut req = ChallengeRequest::clock(true, 180, 2);
    req.variant = "chess960".into();
    let variant = l.mock.send_challenge(Seat::user("Alice", 1500), "damelessbot", &req);
    until("the decline", || l.declined(&variant).is_some()).await;
    assert_eq!(l.declined(&variant).as_deref(), Some("variant"));

    // Blitz is taken, and played: Alice is White.
    let lid = l.mock.send_challenge(Seat::user("Alice", 1500), "damelessbot", &ChallengeRequest::clock(true, 180, 2));
    let id = l.game_of(&lid).await;
    let g = l.game(id).await;
    assert_eq!(g.summary.black.player, PlayerKey::Dameless);
    assert_eq!(g.summary.black.name, "Dameless v1");
    assert_eq!(g.summary.white.player, PlayerKey::Lichess("alice".into()));
    assert!(g.actions.is_empty(), "you do not play the bot's games");
    // A second challenge while one game runs: busy.
    let busy = l.mock.send_challenge(Seat::user("Bob", 1500), "damelessbot", &ChallengeRequest::clock(true, 300, 0));
    until("the decline", || l.declined(&busy).is_some()).await;
    assert_eq!(l.declined(&busy).as_deref(), Some("later"));

    l.mock.opponent_move(&lid, "e2e4");
    until("the bot's reply", || l.mock.game(&lid).unwrap().moves.len() == 2).await;
    l.mock.opponent_move(&lid, "g1f3");
    until("the bot's reply", || l.mock.game(&lid).unwrap().moves.len() == 4).await;
    until_async!("the record to follow", l.game(id).await.moves.len() == 4);
    let g = l.game(id).await;
    assert!(g.moves[1].eval.is_some() && g.moves[3].eval.is_some(), "the bot's moves carry its scores");
    assert!(g.moves[0].eval.is_none(), "Alice's carry none");

    // Alice leaves: the bot claims the game.
    l.mock.opponent_gone(&lid, LichessSide::White, 0);
    let g = l.s.games.wait_finished(id, Duration::from_secs(5)).await.unwrap();
    assert_eq!((g.summary.outcome, g.summary.ending), (Some(Outcome::BlackWins), Some(Ending::Abandoned)));
    assert_eq!(l.mock.game(&lid).unwrap().status, LichessStatus::Timeout);
    until("the cores back", || l.s.hub.ledger.snapshot().reserved == 0).await;

    let events = drain(&mut events);
    let challenges: Vec<_> = events.iter().filter(|e| e.kind == EventKind::Challenge).collect();
    assert!(challenges.iter().any(|e| e.data["event"] == "declined" && e.data["reason"] == "timeControl"));
    assert!(challenges.iter().any(|e| e.data["event"] == "accepted"));
    assert!(count(&events, EventKind::GameThinking) > 0);
}

#[tokio::test]
async fn the_bot_resigns_when_its_engine_dies() {
    let binaries = FixedBinaries::empty().with("champion", fake(), &["--die"]).shared();
    let l = lichess(binaries).await;
    let lid = l.mock.send_challenge(Seat::user("Alice", 1500), "damelessbot", &ChallengeRequest::clock(true, 180, 2));
    let id = l.game_of(&lid).await;
    l.mock.opponent_move(&lid, "e2e4");
    let g = l.s.games.wait_finished(id, Duration::from_secs(10)).await.unwrap();
    // One move each was not played yet: Lichess lets the game be aborted.
    assert_eq!(g.summary.ending, Some(Ending::Aborted));
    assert_eq!(l.mock.game(&lid).unwrap().status, LichessStatus::Aborted);
}

#[tokio::test]
async fn the_bot_seeks_other_bots_until_one_accepts() {
    let l = lichess(binaries()).await;
    l.mock.set_online_bots(vec![
        json!({ "id": "damelessbot", "username": "DamelessBot", "title": "BOT", "perfs": { "blitz": { "rating": 2000 } } }),
        json!({ "id": "alpha", "username": "Alpha", "title": "BOT", "perfs": { "blitz": { "rating": 2050 } } }),
        json!({ "id": "beta", "username": "Beta", "title": "BOT", "perfs": { "blitz": { "rating": 1950 } } }),
    ]);
    l.mock.answer_challenges("alpha", ChallengeAnswer::Decline(DeclineReason::TooFast));
    l.mock.answer_challenges("beta", ChallengeAnswer::Accept);
    let status = l.connections.bot().set_seek(SeekMode::Once).await.unwrap();
    assert_eq!(status.bot.unwrap().seek.mode, SeekMode::Once);
    until("a game against Beta", || {
        l.s.games.live().iter().any(|g| {
            g.white.player == PlayerKey::Lichess("beta".into()) || g.black.player == PlayerKey::Lichess("beta".into())
        })
    })
    .await;
    until("the search to stop", || l.connections.status(ConnectionKey::Bot).bot.unwrap().seek.mode == SeekMode::Off)
        .await;
    let bot = l.connections.status(ConnectionKey::Bot).bot.unwrap();
    assert!(bot.seek.note.unwrap().contains("Beta accepted"));
    assert_eq!(bot.bot_games_today, 1, "a game against a bot counts");
    // Never ourselves.
    assert!(l.mock.requests_to("/api/challenge/DamelessBot").is_empty());
}

#[tokio::test]
async fn the_daily_limit_stops_bot_games_but_not_people() {
    let l = lichess(binaries()).await;
    l.mock.set_online_bots(vec![
        json!({ "id": "alpha", "username": "Alpha", "title": "BOT", "perfs": { "blitz": { "rating": 2050 } } }),
    ]);
    l.mock.bot_daily_limit_next("/api/challenge/Alpha", true, 3600);
    l.connections.bot().set_seek(SeekMode::Always).await.unwrap();
    until("the limit", || l.connections.status(ConnectionKey::Bot).bot.unwrap().bot_limit_until.is_some()).await;
    until("the note", || {
        l.connections
            .status(ConnectionKey::Bot)
            .bot
            .unwrap()
            .seek
            .note
            .is_some_and(|n| n.starts_with("Played 100 games"))
    })
    .await;
    let bot = l.connections.status(ConnectionKey::Bot).bot.unwrap();
    assert_eq!(bot.seek.mode, SeekMode::Always, "the search stays on and goes on when the limit lifts");
    assert!(bot.seek.pending.is_none());

    // A bot's challenge is declined as busy, with why; a person's is taken.
    let from_bot = l.mock.send_challenge(
        Seat::titled("Gamma", 1900, "BOT"),
        "damelessbot",
        &ChallengeRequest::clock(true, 180, 2),
    );
    until("the decline", || l.declined(&from_bot).is_some()).await;
    assert_eq!(l.declined(&from_bot).as_deref(), Some("later"));
    let person =
        l.mock.send_challenge(Seat::user("Alice", 1500), "damelessbot", &ChallengeRequest::clock(true, 180, 2));
    l.game_of(&person).await;

    // Stopping the search keeps the count, and going offline stops everything.
    let status = l.connections.bot().set_online(false).await.unwrap();
    let bot = status.bot.unwrap();
    assert!(!bot.online);
    assert_eq!(bot.seek.mode, SeekMode::Off);
    assert!(bot.bot_limit_until.is_some());
}

#[tokio::test]
async fn settings_are_checked_and_kept() {
    let l = lichess(binaries()).await;
    let bot = l.connections.bot();
    let bad = BotSettings { version: Some("nope".into()), ..BotSettings::default() };
    assert_eq!(bot.set_settings(bad).await.unwrap_err().status(), 404);
    let bad = BotSettings { seek_clocks: vec!["0.25+0".into()], ..BotSettings::default() };
    assert_eq!(bot.set_settings(bad).await.unwrap_err().status(), 400);
    let stored: Option<BotSettings> =
        l.s.hub.store.connection_field(ConnectionKey::Bot, dameless_store::ConnectionField::Settings).await.unwrap();
    assert_eq!(stored.unwrap().version.as_deref(), Some("champion"));
    assert!(l.connections.status(ConnectionKey::Bot).bot.unwrap().online);
    // The bot's challenge to a player, while online.
    let sent = l
        .connections
        .challenge(
            ConnectionKey::Bot,
            dameless_play::you::ChallengeBody {
                username: Some("carol".into()),
                time: Some(dameless_core::TimeControl::clock(180_000, 2_000)),
                ..Default::default()
            },
        )
        .await
        .unwrap();
    assert_eq!(sent.opponent, "carol");
    assert_eq!(l.mock.requests_to("/api/challenge/carol")[0].form["clock.limit"], "180");
}
