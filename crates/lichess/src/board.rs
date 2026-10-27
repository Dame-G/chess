//! The Board API: a person's games played through this app. The game stream says what happens; moves, offers,
//! takebacks, resigning, aborting and claims go back by POST. A seek is a connection Lichess holds open until someone
//! takes it: the game then starts on the event stream, and dropping the connection cancels the seek.

use reqwest::Method;
use serde::Deserialize;
use serde_json::Value;

use crate::{
    Color, Error, Lichess, Ndjson,
    client::{Body, seg, with_query},
    events::GameInfo,
    game::{ChatRoom, GameEvent},
};

/// The Board API of a client whose token is a person's (scope `board:play`).
#[derive(Clone, Copy)]
pub struct Board<'a>(&'a Lichess);

impl Lichess {
    pub fn board(&self) -> Board<'_> {
        Board(self)
    }

    /// The games the account is playing now, up to `nb` (Lichess allows 50), wherever they are played.
    pub async fn ongoing_games(&self, nb: u32) -> Result<Vec<GameInfo>, Error> {
        #[derive(Deserialize)]
        #[serde(rename_all = "camelCase")]
        struct Playing {
            #[serde(default)]
            now_playing: Vec<GameInfo>,
        }
        self.need_token()?;
        let playing: Playing =
            self.get(&with_query("/api/account/playing", &[("nb", nb.clamp(1, 50).to_string())])).await?;
        Ok(playing.now_playing)
    }
}

/// A seek: the kind of game wanted.
#[derive(Clone, Debug, PartialEq)]
pub struct SeekRequest {
    pub rated: bool,
    pub time: SeekTime,
    /// The colour we play (Lichess may ignore it for rated games).
    pub color: Color,
    pub variant: String,
    /// Only opponents rated within this range, such as (1500, 1800).
    pub rating_range: Option<(u32, u32)>,
}

/// A seek's clock. Through the Board API, real-time seeks are rapid or slower.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum SeekTime {
    Clock { limit_seconds: u32, increment_seconds: u32 },
    Correspondence { days: u32 },
}

impl SeekRequest {
    pub fn clock(rated: bool, limit_seconds: u32, increment_seconds: u32) -> Self {
        Self {
            rated,
            time: SeekTime::Clock { limit_seconds, increment_seconds },
            color: Color::Random,
            variant: "standard".into(),
            rating_range: None,
        }
    }

    fn form(&self) -> Vec<(&'static str, String)> {
        let mut form = vec![("rated", self.rated.to_string()), ("variant", self.variant.clone())];
        match self.time {
            // Minutes, which Lichess takes as a decimal.
            SeekTime::Clock { limit_seconds, increment_seconds } => {
                form.push(("time", format!("{}", f64::from(limit_seconds) / 60.0)));
                form.push(("increment", increment_seconds.to_string()));
            }
            SeekTime::Correspondence { days } => form.push(("days", days.to_string())),
        }
        if self.color != Color::Random {
            form.push(("color", self.color.key().to_owned()));
        }
        if let Some((low, high)) = self.rating_range {
            form.push(("ratingRange", format!("{low}-{high}")));
        }
        form
    }
}

/// An open seek. It lasts as long as this value: drop it to cancel the seek at Lichess.
pub struct Seek {
    stream: Ndjson<Value>,
    /// A correspondence seek's id (it stays in the lobby after the connection ends).
    pub id: Option<String>,
}

impl Seek {
    /// Waits until Lichess closes the seek: someone took it (the game starts on the event stream) or it expired. An
    /// error means the connection was lost, and the seek with it.
    pub async fn closed(&mut self) -> Result<(), Error> {
        loop {
            match self.stream.next().await {
                Ok(Some(v)) => {
                    if let Some(id) = v["id"].as_str() {
                        self.id = Some(id.to_owned());
                    }
                }
                Ok(None) => return Ok(()),
                Err(Error::Unexpected(_)) => {}
                Err(e) => return Err(e),
            }
        }
    }
}

impl Board<'_> {
    fn path(id: &str, action: &str) -> String {
        format!("/api/board/game/{}/{action}", seg(id))
    }

    async fn post(&self, id: &str, action: &str) -> Result<(), Error> {
        self.0.need_token()?;
        self.0.post_ok(&Self::path(id, action), Vec::new()).await
    }

    /// Opens a game's stream: `gameFull`, then every change until the game ends.
    pub async fn stream_game(&self, id: &str) -> Result<Ndjson<GameEvent>, Error> {
        self.0.need_token()?;
        self.0.stream(Method::GET, &format!("/api/board/game/stream/{}", seg(id)), Body::None).await
    }

    /// Plays a move (UCI). Lichess answers once it has the move; the game stream then shows it.
    pub async fn make_move(&self, id: &str, uci: &str) -> Result<(), Error> {
        self.post(id, &format!("move/{}", seg(uci))).await
    }

    pub async fn resign(&self, id: &str) -> Result<(), Error> {
        self.post(id, "resign").await
    }

    pub async fn abort(&self, id: &str) -> Result<(), Error> {
        self.post(id, "abort").await
    }

    /// Offers or accepts a draw (`yes`), or declines the opponent's offer (`no`).
    pub async fn draw(&self, id: &str, yes: bool) -> Result<(), Error> {
        self.post(id, if yes { "draw/yes" } else { "draw/no" }).await
    }

    /// Proposes or accepts a takeback (`yes`), or declines the opponent's proposal (`no`).
    pub async fn takeback(&self, id: &str, yes: bool) -> Result<(), Error> {
        self.post(id, if yes { "takeback/yes" } else { "takeback/no" }).await
    }

    /// Claims the win once the opponent has left long enough.
    pub async fn claim_victory(&self, id: &str) -> Result<(), Error> {
        self.post(id, "claim-victory").await
    }

    /// Claims a draw once the opponent has left long enough.
    pub async fn claim_draw(&self, id: &str) -> Result<(), Error> {
        self.post(id, "claim-draw").await
    }

    pub async fn chat(&self, id: &str, room: ChatRoom, text: &str) -> Result<(), Error> {
        self.0.need_token()?;
        let form = vec![("room", room.key().to_owned()), ("text", text.to_owned())];
        self.0.post_ok(&Self::path(id, "chat"), form).await
    }

    /// Opens a seek. It stays open while the returned [`Seek`] lives.
    pub async fn seek(&self, req: &SeekRequest) -> Result<Seek, Error> {
        self.0.need_token()?;
        let stream = self.0.stream(Method::POST, "/api/board/seek", Body::Form(req.form())).await?;
        Ok(Seek { stream, id: None })
    }
}

#[cfg(test)]
mod tests {
    use std::time::Duration;

    use super::*;
    use crate::{
        GameStatus, Side,
        events::Event,
        game::GameState,
        testing::{GameSetup, MockAccount, MockLichess, Reply, Seat},
    };

    async fn next_state(stream: &mut Ndjson<GameEvent>) -> GameState {
        loop {
            match stream.next().await.unwrap() {
                Some(GameEvent::GameState(s)) => return s,
                Some(_) => continue,
                None => panic!("the stream ended"),
            }
        }
    }

    #[tokio::test]
    async fn moves_offers_and_takebacks() {
        let mock = MockLichess::start().await;
        mock.add_account("me", MockAccount::user("Dame"));
        let id = mock.start_game(
            GameSetup::new(Seat::account("dame"), Seat::user("Alice", 1700)).clock(600, 5).reply(Reply::FirstLegal),
        );
        let client = mock.client(Some("me"));
        let board = client.board();
        let mut stream = board.stream_game(&id).await.unwrap();
        let Some(GameEvent::GameFull(full)) = stream.next().await.unwrap() else { panic!() };
        assert_eq!(full.speed.as_deref(), Some("rapid"));
        board.make_move(&id, "e2e4").await.unwrap();
        next_state(&mut stream).await;
        assert_eq!(next_state(&mut stream).await.plies(), 2);

        // Alice offers a draw; we decline it, then she proposes a takeback of her move and we accept.
        mock.offer_draw(&id, Side::Black);
        assert_eq!(next_state(&mut stream).await.draw_offer(), Some(Side::Black));
        board.draw(&id, false).await.unwrap();
        assert_eq!(next_state(&mut stream).await.draw_offer(), None);
        mock.offer_takeback(&id, Side::Black);
        assert_eq!(next_state(&mut stream).await.takeback_offer(), Some(Side::Black));
        board.takeback(&id, true).await.unwrap();
        assert_eq!(next_state(&mut stream).await.moves, "e2e4");

        // Our draw offer stands until Alice answers.
        board.draw(&id, true).await.unwrap();
        assert_eq!(next_state(&mut stream).await.draw_offer(), Some(Side::White));
        mock.opponent_move(&id, "e7e5");
        let s = next_state(&mut stream).await;
        assert_eq!((s.plies(), s.draw_offer()), (2, None));
        assert_eq!(board.abort(&id).await.unwrap_err().status(), Some(400));
        board.resign(&id).await.unwrap();
        let end = next_state(&mut stream).await;
        assert_eq!((end.status, end.winner), (GameStatus::Resign, Some(Side::Black)));
    }

    #[tokio::test]
    async fn a_takeback_we_propose_waits_for_the_opponent() {
        let mock = MockLichess::start().await;
        mock.add_account("me", MockAccount::user("Dame"));
        let id = mock.start_game(GameSetup::new(Seat::account("dame"), Seat::user("Alice", 1700)).clock(600, 0));
        let board_client = mock.client(Some("me"));
        board_client.board().make_move(&id, "d2d4").await.unwrap();
        board_client.board().takeback(&id, true).await.unwrap();
        assert_eq!(mock.game(&id).unwrap().takeback_offer, Some(Side::White));
        mock.accept_takeback(&id);
        assert!(mock.game(&id).unwrap().moves.is_empty());
    }

    #[tokio::test]
    async fn claims_a_draw_after_the_opponent_left() {
        let mock = MockLichess::start().await;
        mock.add_account("me", MockAccount::user("Dame"));
        let id = mock.start_game(GameSetup::new(Seat::account("dame"), Seat::user("Alice", 1700)).clock(600, 0));
        let client = mock.client(Some("me"));
        client.board().make_move(&id, "e2e4").await.unwrap();
        mock.opponent_move(&id, "e7e5");
        mock.opponent_gone(&id, Side::Black, 0);
        client.board().claim_draw(&id).await.unwrap();
        let g = mock.game(&id).unwrap();
        assert_eq!((g.status, g.winner), (GameStatus::Draw, None));
    }

    #[tokio::test]
    async fn a_seek_resolves_into_a_game_and_dropping_it_cancels() {
        let mock = MockLichess::start().await;
        mock.add_account("me", MockAccount::user("Dame"));
        let client = mock.client(Some("me"));
        let mut events = client.events().await.unwrap();
        let mut req = SeekRequest::clock(true, 600, 5);
        req.rating_range = Some((1500, 1800));
        req.color = Color::Black;
        let mut seek = client.board().seek(&req).await.unwrap();
        let open = mock.seeks();
        assert_eq!(open.len(), 1);
        assert!(open[0].open);
        assert_eq!(open[0].form["time"], "10");
        assert_eq!(open[0].form["increment"], "5");
        assert_eq!(open[0].form["ratingRange"], "1500-1800");
        assert_eq!(open[0].form["color"], "black");
        let game = mock.take_seek(&open[0].id, Seat::user("Bob", 1650));
        tokio::time::timeout(Duration::from_secs(5), seek.closed()).await.unwrap().unwrap();
        let Some(Event::GameStart { game: info }) = events.next().await.unwrap() else { panic!() };
        assert_eq!((info.id(), info.color), (game.as_str(), Some(Side::Black)));
        assert_eq!(client.ongoing_games(10).await.unwrap().len(), 1);

        let seek = client.board().seek(&SeekRequest::clock(false, 900, 10)).await.unwrap();
        assert!(mock.seeks().iter().any(|s| s.open));
        drop(seek);
        mock.until("the seek is cancelled", |m| m.seeks().iter().all(|s| !s.open)).await;
        assert!(mock.seeks().iter().any(|s| s.canceled));
    }
}
