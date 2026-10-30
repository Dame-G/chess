//! Profiles and comparisons over stored games and the reviews the fake engine made of them.

mod common;

use chess_analysis::{Profiles, ReviewRequest, Reviews};
use chess_core::{
    PlayerKey, Side, Speed, chess::Position, game::Outcome, profile::ProfileFilter, review::judge::Judgement,
    work::WorkKind,
};
use chess_hub::{Executor, WorkContext};
use common::{QUIET_DRAW, SCHOLARS_MATE, fake, finished_game, harness};

/// You (White) give your queen away on move 3.
const QUEEN_BLUNDER: [&str; 6] = ["e4", "e5", "Qh5", "Nc6", "Qxe5+", "Nxe5"];

#[tokio::test]
async fn profiles_add_up_games_and_reviews() {
    let h = harness(2).await;
    let mate = finished_game(&h.hub, PlayerKey::You, fake(), &SCHOLARS_MATE, Outcome::WhiteWins, 1).await;
    let draw = finished_game(&h.hub, fake(), PlayerKey::You, &QUIET_DRAW, Outcome::Draw, 2).await;
    let bob = PlayerKey::lichess("Bob").unwrap();
    let blunder = finished_game(&h.hub, PlayerKey::You, bob.clone(), &QUEEN_BLUNDER, Outcome::BlackWins, 3).await;
    finished_game(&h.hub, PlayerKey::You, PlayerKey::Engine("maia-1500".into()), &QUIET_DRAW, Outcome::Draw, 4).await;

    let reviews = Reviews::new(h.eval.clone());
    let new = reviews.prepare(ReviewRequest { games: vec![mate, draw, blunder], ..Default::default() }).await.unwrap();
    let spec = new.spec.clone();
    let work = h.hub.store.insert_work(new).await.unwrap();
    let (cx, _cancel) = WorkContext::detached(h.hub.clone(), work.id, WorkKind::Review, spec);
    reviews.run(cx).await.unwrap();

    let profiles = Profiles::new(h.hub.clone());
    let you = profiles.profile(PlayerKey::You, ProfileFilter::default()).await.unwrap();
    assert_eq!(you.player.name, "You");
    assert_eq!((you.games, you.reviewed, you.pending_review), (4, 3, 1));
    let r = &you.results.overall;
    assert_eq!((r.wins, r.draws, r.losses), (1, 2, 1));
    assert!(you.accuracy.overall.is_some());
    assert_eq!(you.trend.len(), 4);
    assert!(you.moments.blunders.iter().any(|m| m.game == blunder && m.san == "Qxe5+"));

    // Weak spots are yours, each the position before your mistake, the queen first among them.
    let spot = you.weak_spots.iter().find(|s| s.game == blunder).expect("the queen blunder is a weak spot");
    assert_eq!((spot.ply, spot.played.as_str()), (5, "Qxe5+"));
    assert_eq!(spot.fen, Position::after(None, &QUEEN_BLUNDER[..4]).unwrap().fen());
    for spot in &you.weak_spots {
        let yours = if spot.game == draw { Side::Black } else { Side::White };
        assert_eq!(if spot.ply % 2 == 1 { Side::White } else { Side::Black }, yours);
    }
    let review = h.hub.store.review(blunder, None, None).await.unwrap().unwrap();
    let your_mistakes = review
        .plies
        .iter()
        .filter(|p| p.side == Side::White && matches!(p.judgement, Some(Judgement::Mistake | Judgement::Blunder)))
        .count();
    assert_eq!(you.weak_spots.iter().filter(|s| s.game == blunder).count(), your_mistakes);

    // The engine walked into the mate; its profile has no weak spots (those are yours alone).
    let chess = profiles.profile(fake(), ProfileFilter::default()).await.unwrap();
    assert_eq!((chess.player.name.as_str(), chess.games, chess.reviewed), ("Fake", 2, 2));
    assert!(chess.judgements.blunder >= 1);
    assert!(chess.weak_spots.is_empty());

    let player = profiles.player(&bob).await.unwrap();
    assert_eq!((player.name.as_str(), player.lichess.as_deref()), ("lichess:bob", Some("bob")));
    let only_blitz = ProfileFilter { speeds: vec![Speed::Blitz], ..Default::default() };
    assert_eq!(profiles.profile(bob.clone(), only_blitz).await.unwrap().games, 1);
    let only_rapid = ProfileFilter { speeds: vec![Speed::Rapid], ..Default::default() };
    assert_eq!(profiles.profile(bob.clone(), only_rapid).await.unwrap().games, 0);
    assert_eq!(profiles.player(&PlayerKey::Engine("maia-1500".into())).await.unwrap().name, "Maia 1500");
    assert_eq!(profiles.player(&PlayerKey::Engine("reckless".into())).await.unwrap().name, "Reckless");

    let comparison =
        profiles.compare(vec![PlayerKey::You, fake(), bob.clone()], ProfileFilter::default()).await.unwrap();
    assert_eq!(comparison.profiles.len(), 3);
    assert_eq!((comparison.profiles[2].games, comparison.profiles[2].results.overall.wins), (1, 1));
    assert!(comparison.leaders.iter().any(|l| l.row == "accuracy" && !l.players.is_empty()));
    assert_eq!(profiles.compare(vec![PlayerKey::You], ProfileFilter::default()).await.unwrap_err().status(), 400);
    let twice = profiles.compare(vec![bob.clone(), bob], ProfileFilter::default()).await;
    assert_eq!(twice.unwrap_err().status(), 400);
}
