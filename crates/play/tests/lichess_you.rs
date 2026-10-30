//! You on Lichess, against the mock: logging in, your games played through the Board API (moves both ways, offers,
//! takebacks, the opponent leaving and the claim), games elsewhere, the fair-play policy, seeks and challenges.

mod common;

use std::sync::Arc;

use chess_core::{
    Color, ConnectionKey, GameId, PlayerKey, Side, TimeControl,
    events::{Assist, EventKind},
    game::{Ending, Game, GameStatus, Outcome, SeatAction},
    lichess::{ConnectionState, Direction},
};
use chess_lichess::{
    GameStatus as LichessStatus,
    challenge::ChallengeRequest,
    explorer::ExplorerQuery,
    testing::{GameSetup, MockAccount, MockLichess, Reply, Seat},
};
use chess_play::{
    Connections,
    you::{ChallengeBody, SeekRequestBody},
};
use common::*;

struct Lichess {
    s: Setup,
    mock: MockLichess,
    connections: Connections,
}

async fn lichess() -> Lichess {
    let s = setup(engines());
    let mock = MockLichess::start().await;
    mock.add_account("you-tok", MockAccount::user("Dame").rating("rapid", 1800));
    let connections = Connections::new(s.hub.clone(), s.games.clone(), mock.config());
    connections.start().await.unwrap();
    Lichess { s, mock, connections }
}

impl Lichess {
    async fn log_in(&self) {
        let status = self.connections.set_token(ConnectionKey::You, "you-tok").await.unwrap();
        assert!(status.configured);
        assert_eq!(status.account.as_ref().unwrap().username, "Dame");
        until("the event stream", || self.connections.status(ConnectionKey::You).state == ConnectionState::Online)
            .await;
    }

    /// The game here of Lichess game `lichess`, once it is played here: listed, and its first state read from Lichess
    /// (the record is listed a moment before, with no actions yet).
    async fn game_of(&self, lichess: &str) -> GameId {
        let found = || self.s.games.live().into_iter().find(|g| g.source.lichess_id() == Some(lichess)).map(|g| g.id);
        until("the game to be played here", || found().is_some()).await;
        let id = found().unwrap();
        until_async!("your actions in it", !self.game(id).await.actions.is_empty());
        id
    }

    async fn game(&self, id: GameId) -> Game {
        self.s.games.game(id).await.unwrap()
    }
}

#[tokio::test]
async fn your_game_is_played_through_the_board_api() {
    let l = lichess().await;
    let mut events = subscribe(&l.s.hub);
    l.log_in().await;
    let lid = l.mock.start_game(
        GameSetup::new(Seat::account("dame"), Seat::user("Alice", 1700))
            .clock(600, 5)
            .rated(true)
            .reply(Reply::FirstLegal),
    );
    let id = l.game_of(&lid).await;
    assert_eq!(l.s.hub.policy.current().assist, Assist::Off, "analysis is off while you play");
    let g = l.game(id).await;
    assert_eq!(g.summary.you, Some(Side::White));
    assert_eq!(g.summary.white.player, PlayerKey::You);
    assert_eq!(g.summary.white.name, "Dame");
    assert_eq!(g.summary.black.player, PlayerKey::Lichess("alice".into()));
    assert_eq!(g.summary.black.rating, Some(1700));
    assert!(g.summary.url.as_deref().is_some_and(|u| u.ends_with(&lid)));
    assert_eq!(g.actions, [SeatAction::Move, SeatAction::Abort]);
    assert_eq!(l.connections.status(ConnectionKey::You).playing, [id]);

    // Your move goes to Lichess and counts once the stream shows it; Alice answers.
    assert_eq!(l.s.games.act(id, SeatAction::Move, Some("e2e5".into())).await.unwrap_err().status(), 400);
    let g = l.s.games.act(id, SeatAction::Move, Some("e4".into())).await.unwrap();
    assert_eq!(g.moves[0].uci, "e2e4");
    until_async!("Alice's reply", l.game(id).await.moves.len() == 2);
    assert_eq!(l.mock.game(&lid).unwrap().moves.len(), 2);

    // Her draw offer, declined.
    l.mock.offer_draw(&lid, Side::Black.into_lichess());
    until_async!("the offer", l.game(id).await.offers.draw == Some(Side::Black));
    assert!(l.game(id).await.actions.contains(&SeatAction::AcceptDraw));
    l.s.games.act(id, SeatAction::DeclineDraw, None).await.unwrap();
    until_async!("the offer to go", l.game(id).await.offers.draw.is_none());

    // Our takeback, accepted by her: the moves go here too.
    l.s.games.act(id, SeatAction::Takeback, None).await.unwrap();
    assert_eq!(l.mock.game(&lid).unwrap().takeback_offer, Some(chess_lichess::Side::White));
    l.mock.accept_takeback(&lid);
    until_async!("the takeback", l.game(id).await.moves.is_empty());
    assert!(l.s.hub.store.game(id).await.unwrap().unwrap().moves.is_empty());

    l.s.games.act(id, SeatAction::Move, Some("d2d4".into())).await.unwrap();
    until_async!("Alice's reply", l.game(id).await.moves.len() == 2);
    // A draw offer of ours stands at Lichess.
    l.s.games.act(id, SeatAction::OfferDraw, None).await.unwrap();
    assert_eq!(l.mock.game(&lid).unwrap().draw_offer, Some(chess_lichess::Side::White));

    // She leaves: the claim is yours to make.
    l.mock.opponent_gone(&lid, chess_lichess::Side::Black, 0);
    until_async!("the claim", l.game(id).await.actions.contains(&SeatAction::ClaimVictory));
    l.s.games.act(id, SeatAction::ClaimVictory, None).await.unwrap();
    let g = l.s.games.wait_finished(id, std::time::Duration::from_secs(5)).await.unwrap();
    assert_eq!(g.summary.status, GameStatus::Finished);
    assert_eq!((g.summary.outcome, g.summary.ending), (Some(Outcome::WhiteWins), Some(Ending::Abandoned)));
    assert_eq!(g.moves.len(), 2);
    until("the policy to lift", || l.s.hub.policy.current().assist == Assist::Allowed).await;
    until("the game to leave the status", || l.connections.status(ConnectionKey::You).playing.is_empty()).await;

    let events = drain(&mut events);
    assert!(count(&events, EventKind::GameMoved) >= 4);
    assert_eq!(count(&events, EventKind::GameUndone), 1);
    assert!(count(&events, EventKind::PolicyChanged) >= 2);
    assert!(count(&events, EventKind::ConnectionUpserted) > 0);
    let gone = events.iter().filter(|e| e.kind == EventKind::GameChanged).any(|e| e.data["opponent_gone"].is_object());
    assert!(gone, "game.changed tells of the opponent leaving");
}

trait IntoLichess {
    fn into_lichess(self) -> chess_lichess::Side;
}

impl IntoLichess for Side {
    fn into_lichess(self) -> chess_lichess::Side {
        match self {
            Side::White => chess_lichess::Side::White,
            Side::Black => chess_lichess::Side::Black,
        }
    }
}

#[tokio::test]
async fn a_game_ending_on_lichess_ends_here_and_you_resign_by_stopping() {
    let l = lichess().await;
    l.log_in().await;
    // Black this time: Alice opens.
    let lid = l.mock.start_game(GameSetup::new(Seat::user("Alice", 1700), Seat::account("dame")).clock(900, 10));
    let id = l.game_of(&lid).await;
    l.mock.opponent_move(&lid, "e2e4");
    until_async!("her move", l.game(id).await.moves.len() == 1);
    let g = l.game(id).await;
    assert_eq!(g.summary.you, Some(Side::Black));
    assert_eq!(g.actions, [SeatAction::Move, SeatAction::Abort]);
    l.s.games.act(id, SeatAction::Move, Some("c7c5".into())).await.unwrap();
    // Flagged on lichess.org.
    l.mock.end_game(&lid, LichessStatus::Outoftime, Some(chess_lichess::Side::Black));
    let g = l.s.games.wait_finished(id, std::time::Duration::from_secs(5)).await.unwrap();
    assert_eq!((g.summary.outcome, g.summary.ending), (Some(Outcome::BlackWins), Some(Ending::Timeout)));

    // Stopping a game that has got going resigns it.
    let lid = l.mock.start_game(
        GameSetup::new(Seat::account("dame"), Seat::user("Bob", 1500)).clock(600, 0).reply(Reply::FirstLegal),
    );
    let id = l.game_of(&lid).await;
    l.s.games.act(id, SeatAction::Move, Some("e2e4".into())).await.unwrap();
    until_async!("Bob's reply", l.game(id).await.moves.len() == 2);
    let g = l.s.games.stop(id).await.unwrap();
    assert_eq!((g.summary.outcome, g.summary.ending), (Some(Outcome::BlackWins), Some(Ending::Resignation)));
    assert_eq!(l.mock.game(&lid).unwrap().status, LichessStatus::Resign);
}

#[tokio::test]
async fn games_elsewhere_and_correspondence_follow_the_policy() {
    let l = lichess().await;
    l.log_in().await;
    // Bullet cannot be played through the Board API: it is played on lichess.org and still turns analysis off.
    let bullet = l.mock.start_game(GameSetup::new(Seat::account("dame"), Seat::user("Alice", 1700)).clock(60, 0));
    until("the game elsewhere", || {
        l.connections.status(ConnectionKey::You).you.unwrap().elsewhere.iter().any(|g| g.id == bullet)
    })
    .await;
    assert_eq!(l.s.hub.policy.current().assist, Assist::Off);
    assert!(l.s.games.live().is_empty());
    assert_eq!(l.connections.tablebase("8/8/8/8/8/2k5/2q5/K7 b - - 0 1").await.unwrap_err().status(), 409);
    l.mock.end_game(&bullet, LichessStatus::Resign, Some(chess_lichess::Side::White));
    until("the policy to lift", || l.s.hub.policy.current().assist == Assist::Allowed).await;
    assert!(l.connections.status(ConnectionKey::You).you.unwrap().elsewhere.is_empty());

    // Correspondence allows the explorer, nothing else.
    l.mock.set_explorer("lichess", vec![serde_json::json!({ "white": 3, "draws": 1, "black": 2, "moves": [] })]);
    l.mock.set_tablebase(serde_json::json!({ "category": "win" }));
    let corr = l.mock.start_game(GameSetup::new(Seat::account("dame"), Seat::user("Carol", 1600)).correspondence(3));
    let id = l.game_of(&corr).await;
    until("explorer only", || l.s.hub.policy.current().assist == Assist::ExplorerOnly).await;
    assert_eq!(l.game(id).await.summary.time, TimeControl::Correspondence { days: 3 });
    assert_eq!(l.connections.explorer(&ExplorerQuery::default()).await.unwrap()["white"], 3);
    assert_eq!(l.connections.tablebase("8/8/8/8/8/2k5/2q5/K7 b - - 0 1").await.unwrap_err().status(), 409);
    l.s.games.act(id, SeatAction::Abort, None).await.unwrap();
    l.s.games.wait_finished(id, std::time::Duration::from_secs(5)).await.unwrap();
    until("the policy to lift", || l.s.hub.policy.current().assist == Assist::Allowed).await;
    assert_eq!(l.connections.tablebase("8/8/8/8/8/2k5/2q5/K7 b - - 0 1").await.unwrap()["category"], "win");
}

#[tokio::test]
async fn seeks_and_challenges() {
    let l = lichess().await;
    l.log_in().await;
    let you = l.connections.you();

    // Blitz cannot be sought through the Board API.
    let blitz = SeekRequestBody {
        time: TimeControl::clock(300_000, 3_000),
        rated: true,
        color: Color::Random,
        rating_range: None,
    };
    assert_eq!(you.seek(blitz).await.unwrap_err().status(), 400);
    let rapid = SeekRequestBody {
        time: TimeControl::clock(600_000, 5_000),
        rated: true,
        color: Color::Black,
        rating_range: Some("1500-1900".into()),
    };
    let status = you.seek(rapid.clone()).await.unwrap();
    assert!(status.you.unwrap().seek.is_some());
    until("the seek at Lichess", || l.mock.seeks().iter().any(|s| s.open)).await;
    let open = l.mock.seeks().into_iter().find(|s| s.open).unwrap();
    assert_eq!((open.form["time"].as_str(), open.form["color"].as_str()), ("10", "black"));
    let lid = l.mock.take_seek(&open.id, Seat::user("Bob", 1650));
    let id = l.game_of(&lid).await;
    assert_eq!(l.game(id).await.summary.you, Some(Side::Black));
    until("the seek to close", || l.connections.status(ConnectionKey::You).you.unwrap().seek.is_none()).await;
    l.s.games.stop(id).await.unwrap();

    // A seek cancelled.
    you.seek(rapid).await.unwrap();
    until("the seek at Lichess", || l.mock.seeks().iter().any(|s| s.open)).await;
    you.cancel_seek().await;
    until("the seek to be cancelled", || l.mock.seeks().iter().all(|s| !s.open)).await;

    // Stockfish answers at once.
    let ai = ChallengeBody {
        ai_level: Some(3),
        time: Some(TimeControl::clock(600_000, 0)),
        color: Color::White,
        ..ChallengeBody::default()
    };
    let sent = l.connections.challenge(ConnectionKey::You, ai).await.unwrap();
    let id = l.game_of(&sent.id).await;
    let black = l.game(id).await.summary.black;
    assert_eq!((black.player, black.level), (PlayerKey::Engine("stockfish".into()), Some(3)));
    l.s.games.act(id, SeatAction::Move, Some("e2e4".into())).await.unwrap();
    until_async!("Stockfish's reply", l.game(id).await.moves.len() == 2);
    l.s.games.stop(id).await.unwrap();

    // Bullet challenges are refused; a player is challenged and the challenge withdrawn.
    let bullet = ChallengeBody {
        username: Some("carol".into()),
        time: Some(TimeControl::clock(60_000, 0)),
        ..ChallengeBody::default()
    };
    assert_eq!(l.connections.challenge(ConnectionKey::You, bullet).await.unwrap_err().status(), 400);
    let carol = ChallengeBody {
        username: Some("carol".into()),
        time: Some(TimeControl::clock(300_000, 3_000)),
        rated: true,
        ..ChallengeBody::default()
    };
    let sent = l.connections.challenge(ConnectionKey::You, carol).await.unwrap();
    until("our challenge in the status", || {
        l.connections
            .status(ConnectionKey::You)
            .challenges
            .iter()
            .any(|c| c.id == sent.id && c.direction == Direction::Outgoing)
    })
    .await;
    you.cancel(&sent.id).await.unwrap();
    assert!(l.connections.status(ConnectionKey::You).challenges.is_empty());

    // Challenges to you: one declined with a reason, one accepted.
    let later = l.mock.send_challenge(Seat::user("Bob", 1500), "dame", &ChallengeRequest::clock(true, 600, 5));
    let take = l.mock.send_challenge(Seat::user("Eve", 1550), "dame", &ChallengeRequest::clock(false, 900, 10));
    until("the challenges", || l.connections.status(ConnectionKey::You).challenges.len() == 2).await;
    assert_eq!(you.decline(&later, Some("nope")).await.unwrap_err().status(), 400);
    you.decline(&later, Some("later")).await.unwrap();
    assert_eq!(l.mock.requests_to(&format!("/api/challenge/{later}/decline"))[0].form["reason"], "later");
    you.accept(&take).await.unwrap();
    let id = l.game_of(&take).await;
    assert_eq!(l.game(id).await.summary.white.player, PlayerKey::Lichess("eve".into()));
}

#[tokio::test]
async fn tokens_are_checked_and_logins_go_through_oauth() {
    let l = lichess().await;
    l.mock.add_account("bot-tok", MockAccount::bot("SomeBot"));
    l.mock.add_account("watch-only", MockAccount::user("Dame").scopes(&["challenge:read"]));
    let e = l.connections.set_token(ConnectionKey::You, "bot-tok").await.unwrap_err();
    assert_eq!(e.status(), 400, "a BOT account is not yours");
    let e = l.connections.set_token(ConnectionKey::You, "watch-only").await.unwrap_err();
    assert!(e.message().contains("board:play"), "{e}");
    assert_eq!(l.connections.set_token(ConnectionKey::You, "unknown").await.unwrap_err().status(), 400);
    assert!(!l.connections.status(ConnectionKey::You).configured);

    // A login: the link, Lichess's answer, the token stored.
    l.mock.add_account("lio_granted", MockAccount::user("Dame"));
    let callback = "http://127.0.0.1:7979/api/connections/you/callback";
    assert_eq!(l.connections.start_login(Some("https://evil.example/"), callback).unwrap_err().status(), 400);
    let started = l.connections.start_login(None, callback).unwrap();
    assert_eq!(started.expires_in_seconds, 600);
    let back = l.mock.authorize(&started.url, "lio_granted").unwrap();
    assert_eq!(back.redirect_uri, callback);
    let finished = l.connections.finish_login(Some(&back.code), &back.state, None).await.unwrap();
    assert!(finished.result.is_ok(), "{:?}", finished.result);
    assert_eq!(finished.redirect(), "http://127.0.0.1:7979/you?lichess=connected");
    assert_eq!(l.s.hub.secrets.lichess_token(ConnectionKey::You).unwrap().as_deref(), Some("lio_granted"));
    assert!(l.connections.status(ConnectionKey::You).configured);
    // Single use.
    assert_eq!(l.connections.finish_login(Some(&back.code), &back.state, None).await.unwrap_err().status(), 400);
    // Refused at Lichess: back to the page with the reason.
    let started = l.connections.start_login(Some("http://localhost:5173/you"), callback).unwrap();
    let state =
        reqwest::Url::parse(&started.url).unwrap().query_pairs().find(|(k, _)| k == "state").unwrap().1.into_owned();
    let refused = l.connections.finish_login(None, &state, Some("access_denied")).await.unwrap();
    assert!(refused.redirect().starts_with("http://localhost:5173/you?lichess=error&message="));

    // Logging out revokes the token at Lichess and forgets it.
    let status = l.connections.logout(ConnectionKey::You).await.unwrap();
    assert!(!status.configured);
    assert!(!l.mock.has_token("lio_granted"));
    assert_eq!(l.s.hub.secrets.lichess_token(ConnectionKey::You).unwrap(), None);
    let _ = Arc::clone(&l.s.hub);
}
