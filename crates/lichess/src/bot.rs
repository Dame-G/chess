//! The Bot API: a BOT account's games. The game stream says what happens; moves, resigning, aborting, claims and chat
//! go back by POST. Challenges, the event stream and the online bots are on [`Lichess`] itself.

use reqwest::Method;

use crate::{
    Error, Lichess, Ndjson,
    client::{Body, seg, with_query},
    game::{ChatRoom, GameEvent},
};

/// The Bot API of a client whose token is a BOT account's.
#[derive(Clone, Copy)]
pub struct Bot<'a>(&'a Lichess);

impl Lichess {
    pub fn bot(&self) -> Bot<'_> {
        Bot(self)
    }
}

impl Bot<'_> {
    fn path(id: &str, action: &str) -> String {
        format!("/api/bot/game/{}/{action}", seg(id))
    }

    /// Opens a game's stream: `gameFull`, then every change until the game ends.
    pub async fn stream_game(&self, id: &str) -> Result<Ndjson<GameEvent>, Error> {
        self.0.need_token()?;
        self.0.stream(Method::GET, &format!("/api/bot/game/stream/{}", seg(id)), Body::None).await
    }

    /// Plays a move (UCI), offering a draw with it when `offering_draw`.
    pub async fn make_move(&self, id: &str, uci: &str, offering_draw: bool) -> Result<(), Error> {
        self.0.need_token()?;
        let mut path = Self::path(id, &format!("move/{}", seg(uci)));
        if offering_draw {
            path = with_query(&path, &[("offeringDraw", "true".into())]);
        }
        self.0.post_ok(&path, Vec::new()).await
    }

    pub async fn resign(&self, id: &str) -> Result<(), Error> {
        self.0.need_token()?;
        self.0.post_ok(&Self::path(id, "resign"), Vec::new()).await
    }

    /// Aborts a game nobody has moved in yet (each side may abort before its first move).
    pub async fn abort(&self, id: &str) -> Result<(), Error> {
        self.0.need_token()?;
        self.0.post_ok(&Self::path(id, "abort"), Vec::new()).await
    }

    /// Claims the win once the opponent has left long enough (`opponentGone`).
    pub async fn claim_victory(&self, id: &str) -> Result<(), Error> {
        self.0.need_token()?;
        self.0.post_ok(&Self::path(id, "claim-victory"), Vec::new()).await
    }

    /// Claims a draw once the opponent has left long enough.
    pub async fn claim_draw(&self, id: &str) -> Result<(), Error> {
        self.0.need_token()?;
        self.0.post_ok(&Self::path(id, "claim-draw"), Vec::new()).await
    }

    /// Offers or accepts a draw (`accept`), or declines the opponent's offer.
    pub async fn draw(&self, id: &str, accept: bool) -> Result<(), Error> {
        self.0.need_token()?;
        self.0.post_ok(&Self::path(id, if accept { "draw/yes" } else { "draw/no" }), Vec::new()).await
    }

    /// Writes in the game's chat.
    pub async fn chat(&self, id: &str, room: ChatRoom, text: &str) -> Result<(), Error> {
        self.0.need_token()?;
        let form = vec![("room", room.key().to_owned()), ("text", text.to_owned())];
        self.0.post_ok(&Self::path(id, "chat"), form).await
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        GameStatus, Side,
        game::GameEvent,
        testing::{GameSetup, MockAccount, MockLichess, Reply, Seat},
    };

    async fn next_state(stream: &mut Ndjson<GameEvent>) -> crate::game::GameState {
        loop {
            match stream.next().await.unwrap() {
                Some(GameEvent::GameState(s)) => return s,
                Some(_) => continue,
                None => panic!("the stream ended"),
            }
        }
    }

    #[tokio::test]
    async fn plays_a_game_to_the_end() {
        let mock = MockLichess::start().await;
        mock.add_account("bot", MockAccount::bot("RecklessBot"));
        let id = mock.start_game(
            GameSetup::new(Seat::account("recklessbot"), Seat::user("Alice", 1650)).reply(Reply::FirstLegal),
        );
        let bot = mock.client(Some("bot"));
        let mut stream = bot.bot().stream_game(&id).await.unwrap();
        let Some(GameEvent::GameFull(full)) = stream.next().await.unwrap() else { panic!() };
        assert_eq!(full.side_of("recklessbot"), Some(Side::White));
        assert_eq!(full.clock.unwrap().initial, 180_000);
        bot.bot().make_move(&id, "e2e4", false).await.unwrap();
        assert_eq!(next_state(&mut stream).await.moves, "e2e4");
        // Alice answers with her first legal move.
        assert_eq!(next_state(&mut stream).await.plies(), 2);
        // An illegal move is refused.
        let e = bot.bot().make_move(&id, "e2e4", false).await.unwrap_err();
        assert_eq!(e.status(), Some(400));
        bot.bot().chat(&id, ChatRoom::Player, "good luck").await.unwrap();
        assert_eq!(mock.requests_to(&format!("/api/bot/game/{id}/chat"))[0].form["text"], "good luck");
        bot.bot().resign(&id).await.unwrap();
        let end = next_state(&mut stream).await;
        assert_eq!((end.status, end.winner), (GameStatus::Resign, Some(Side::Black)));
        // Lichess closes the stream once the game is over.
        assert!(matches!(stream.next().await, Ok(None)));
    }

    #[tokio::test]
    async fn claims_the_win_when_the_opponent_leaves() {
        let mock = MockLichess::start().await;
        mock.add_account("bot", MockAccount::bot("RecklessBot"));
        let id = mock.start_game(GameSetup::new(Seat::user("Alice", 1650), Seat::account("recklessbot")));
        let bot = mock.client(Some("bot"));
        let mut stream = bot.bot().stream_game(&id).await.unwrap();
        stream.next().await.unwrap();
        assert_eq!(bot.bot().claim_victory(&id).await.unwrap_err().status(), Some(400));
        mock.opponent_move(&id, "d2d4");
        mock.opponent_gone(&id, Side::White, 0);
        loop {
            match stream.next().await.unwrap() {
                Some(GameEvent::OpponentGone(g)) => {
                    assert!(g.gone);
                    assert_eq!(g.claim_win_in_seconds, Some(0));
                    break;
                }
                Some(_) => continue,
                None => panic!(),
            }
        }
        bot.bot().claim_victory(&id).await.unwrap();
        let end = next_state(&mut stream).await;
        assert_eq!((end.status, end.winner), (GameStatus::Timeout, Some(Side::Black)));
    }

    #[tokio::test]
    async fn aborts_before_moves_and_offers_draws_with_a_move() {
        let mock = MockLichess::start().await;
        mock.add_account("bot", MockAccount::bot("RecklessBot"));
        let bot = mock.client(Some("bot"));
        let id = mock.start_game(GameSetup::new(Seat::account("recklessbot"), Seat::user("Alice", 1650)));
        bot.bot().abort(&id).await.unwrap();
        assert_eq!(mock.game(&id).unwrap().status, GameStatus::Aborted);
        let id = mock.start_game(GameSetup::new(Seat::account("recklessbot"), Seat::user("Alice", 1650)));
        bot.bot().make_move(&id, "g1f3", true).await.unwrap();
        let req = &mock.requests_to(&format!("/api/bot/game/{id}/move/g1f3"))[0];
        assert_eq!(req.query["offeringDraw"], "true");
        assert_eq!(mock.game(&id).unwrap().draw_offer, Some(Side::White));
        mock.opponent_move(&id, "g8f6");
        mock.offer_draw(&id, Side::Black);
        bot.bot().draw(&id, true).await.unwrap();
        assert_eq!(mock.game(&id).unwrap().status, GameStatus::Draw);
    }
}
