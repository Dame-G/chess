//! Local games against the fake engine: engine against engine, and you against an engine with every action.

mod common;

use std::time::Duration;

use common::*;
use dameless_core::{
    PlayerKey, Side, TimeControl,
    events::EventKind,
    game::{Ending, GameStatus, Outcome, SeatAction, Source},
};
use dameless_play::{NewGameRequest, SeatRequest, StartFrom};
use dameless_store::NewGame;

fn request(white: &str, black: &str, time: TimeControl) -> NewGameRequest {
    NewGameRequest {
        white: white.parse().unwrap(),
        black: black.parse().unwrap(),
        time,
        from: None,
        hash: None,
        threads: None,
        adjudicate: false,
        seed: None,
    }
}

#[tokio::test]
async fn two_engines_play_a_game_to_its_end() {
    let s = setup(binaries());
    let mut events = subscribe(&s.hub);
    let game = s.games.start(request("dameless", "dameless", TimeControl::Nodes { nodes: 1000 })).await.unwrap();
    let id = game.summary.id;
    assert_eq!(game.summary.status, GameStatus::Live);
    assert_eq!(game.summary.white.player, PlayerKey::Dameless);
    assert_eq!(game.summary.white.name, "v1");
    assert_eq!(game.summary.you, None);
    assert!(game.actions.is_empty(), "you do not play in it");
    // Each engine holds a core while it plays.
    assert_eq!(s.hub.ledger.snapshot().reserved, 2);
    assert_eq!(s.games.live().len(), 1);

    let done = s.games.wait_finished(id, Duration::from_secs(60)).await.unwrap();
    assert_eq!(done.summary.status, GameStatus::Finished, "{:?}", done.summary);
    assert!(done.summary.ending.is_some());
    assert!(!done.moves.is_empty());
    // The engines' own scores, White's, on every move.
    assert!(done.moves.iter().all(|m| m.eval.is_some() && m.depth == Some(1) && m.think_ms.is_some()));
    let stored = s.hub.store.game(id).await.unwrap().unwrap();
    assert_eq!(stored.moves, done.moves);
    assert_eq!(stored.summary.plies as usize, done.moves.len());
    until("the engines to give their cores back", || s.hub.ledger.snapshot().reserved == 0).await;
    assert!(s.games.live().is_empty());

    let events = drain(&mut events);
    assert_eq!(count(&events, EventKind::GameMoved), done.moves.len());
    assert!(count(&events, EventKind::GameThinking) > 0);
    assert!(count(&events, EventKind::GameUpserted) >= 3, "starting, live, finished");
    let last = events.iter().rev().find(|e| e.kind == EventKind::GameUpserted).unwrap();
    assert_eq!(last.data["status"], "finished");
}

#[tokio::test]
async fn you_play_an_engine_with_moves_takebacks_offers_and_resigning() {
    let s = setup(binaries());
    let mut events = subscribe(&s.hub);
    let game = s.games.start(request("you", "dameless", TimeControl::Movetime { ms: 50 })).await.unwrap();
    let id = game.summary.id;
    assert_eq!(game.summary.you, Some(Side::White));
    assert_eq!(game.summary.white.player, PlayerKey::You);
    assert_eq!(game.actions, [SeatAction::Move, SeatAction::OfferDraw, SeatAction::Abort]);

    // An illegal move is refused and changes nothing; a move in SAN is taken.
    let e = s.games.act(id, SeatAction::Move, Some("e2e5".into())).await.unwrap_err();
    assert_eq!(e.status(), 400);
    let e = s.games.act(id, SeatAction::Move, None).await.unwrap_err();
    assert_eq!(e.status(), 400);
    let g = s.games.act(id, SeatAction::Move, Some("e4".into())).await.unwrap();
    assert_eq!(g.moves[0].uci, "e2e4");
    let ready = || async { s.games.game(id).await.unwrap() };
    until_async!("the engine's reply", ready().await.moves.len() == 2);
    let g = ready().await;
    assert!(g.actions.contains(&SeatAction::Takeback));
    assert!(g.actions.contains(&SeatAction::Resign));
    assert!(g.moves[1].eval.is_some());

    // A takeback without a clock: your move and the reply go.
    let g = s.games.act(id, SeatAction::Takeback, None).await.unwrap();
    assert!(g.moves.is_empty());
    assert!(s.hub.store.game(id).await.unwrap().unwrap().moves.is_empty());

    s.games.act(id, SeatAction::Move, Some("d2d4".into())).await.unwrap();
    until_async!("the engine's reply", ready().await.moves.len() == 2);
    // Too early for the engine to take a draw: declined, and the game goes on.
    let g = s.games.act(id, SeatAction::OfferDraw, None).await.unwrap();
    assert_eq!(g.summary.status, GameStatus::Live);
    // Not yours to do.
    assert_eq!(s.games.act(id, SeatAction::ClaimVictory, None).await.unwrap_err().status(), 409);
    assert_eq!(s.games.act(id, SeatAction::AcceptDraw, None).await.unwrap_err().status(), 409);

    s.games.act(id, SeatAction::Resign, None).await.unwrap();
    let g = s.games.wait_finished(id, Duration::from_secs(5)).await.unwrap();
    assert_eq!((g.summary.outcome, g.summary.ending), (Some(Outcome::BlackWins), Some(Ending::Resignation)));
    assert!(g.actions.is_empty());
    assert_eq!(s.games.act(id, SeatAction::Resign, None).await.unwrap_err().status(), 409);
    assert_eq!(s.games.act(dameless_core::GameId(999), SeatAction::Resign, None).await.unwrap_err().status(), 404);

    let events = drain(&mut events);
    assert_eq!(count(&events, EventKind::GameUndone), 1);
    assert!(count(&events, EventKind::GameChanged) > 0);
    let changed = events.iter().find(|e| e.kind == EventKind::GameChanged).unwrap();
    assert!(changed.data["actions"].is_array());
}

#[tokio::test]
async fn your_clock_runs_out() {
    let s = setup(binaries());
    let game = s.games.start(request("you", "dameless", TimeControl::clock(400, 0))).await.unwrap();
    let id = game.summary.id;
    assert_eq!(game.clocks.unwrap().white, 400);
    tokio::time::sleep(Duration::from_millis(100)).await;
    // The clock of the side to move runs down as it is read.
    assert!(s.games.game(id).await.unwrap().clocks.unwrap().white < 400);
    let g = s.games.wait_finished(id, Duration::from_secs(5)).await.unwrap();
    assert_eq!((g.summary.outcome, g.summary.ending), (Some(Outcome::BlackWins), Some(Ending::Timeout)));
}

#[tokio::test]
async fn an_engine_that_dies_or_cheats_loses() {
    let s = setup(binaries());
    let game = s.games.start(request("dameless:dying", "dameless", TimeControl::Nodes { nodes: 10 })).await.unwrap();
    let g = s.games.wait_finished(game.summary.id, Duration::from_secs(10)).await.unwrap();
    assert_eq!((g.summary.outcome, g.summary.ending), (Some(Outcome::BlackWins), Some(Ending::EngineFailure)));

    let game = s.games.start(request("dameless", "dameless:illegal", TimeControl::Nodes { nodes: 10 })).await.unwrap();
    let g = s.games.wait_finished(game.summary.id, Duration::from_secs(10)).await.unwrap();
    assert_eq!((g.summary.outcome, g.summary.ending), (Some(Outcome::WhiteWins), Some(Ending::IllegalMove)));
}

#[tokio::test]
async fn stopping_aborts_an_engine_game_and_games_start_from_a_position() {
    let s = setup(binaries());
    let mut req = request("dameless:slow", "dameless:slow", TimeControl::Movetime { ms: 1000 });
    // Mate in one for White: the game ends with its first move.
    req.from = Some(StartFrom::Position { fen: "6k1/5ppp/8/8/8/8/5PPP/R5K1 w - - 0 1".into(), moves: vec![] });
    let game = s.games.start(req).await.unwrap();
    let g = s.games.wait_finished(game.summary.id, Duration::from_secs(10)).await.unwrap();
    assert_eq!(g.start_fen.as_deref(), Some("6k1/5ppp/8/8/8/8/5PPP/R5K1 w - - 0 1"));
    assert_eq!((g.summary.outcome, g.summary.ending), (Some(Outcome::WhiteWins), Some(Ending::Checkmate)));
    assert_eq!(g.moves[0].san, "Ra8#");

    let game =
        s.games.start(request("dameless:slow", "dameless:slow", TimeControl::Movetime { ms: 1000 })).await.unwrap();
    let g = s.games.stop(game.summary.id).await.unwrap();
    assert_eq!((g.summary.status, g.summary.ending), (GameStatus::Aborted, Some(Ending::Aborted)));
}

#[tokio::test]
async fn games_are_checked_before_they_start() {
    let s = setup(binaries());
    let both = s.games.start(request("you", "you", TimeControl::Movetime { ms: 10 })).await.unwrap_err();
    assert_eq!(both.status(), 400);
    let corr = s.games.start(request("you", "dameless", TimeControl::Correspondence { days: 1 })).await.unwrap_err();
    assert_eq!(corr.status(), 400);
    let none = s.games.start(request("you", "dameless:nope", TimeControl::Movetime { ms: 10 })).await.unwrap_err();
    assert_eq!(none.status(), 404);
    let mut book = request("you", "dameless", TimeControl::Movetime { ms: 10 });
    book.from = Some(StartFrom::Book);
    assert_eq!(s.games.start(book).await.unwrap_err().status(), 409, "no book is installed");
    assert!(s.games.live().is_empty());
    assert_eq!("dameless:v3".parse::<SeatRequest>().unwrap().version(), Some("v3"));
}

#[tokio::test]
async fn local_games_left_live_by_a_stopped_app_are_aborted() {
    let s = setup(binaries());
    let mut new = NewGame::starting(
        dameless_core::game::Seat::new(PlayerKey::You, "You"),
        dameless_core::game::Seat::new(PlayerKey::Dameless, "v1"),
        Source::Local,
        TimeControl::clock(60_000, 0),
        dameless_store::now(),
    );
    new.status = GameStatus::Live;
    let id = s.hub.store.insert_game(new).await.unwrap();
    assert_eq!(s.games.recover().await.unwrap(), 1);
    let g = s.games.game(id).await.unwrap();
    assert_eq!((g.summary.status, g.summary.ending), (GameStatus::Aborted, Some(Ending::Aborted)));
}
