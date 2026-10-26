//! Challenges, the same routes for a bot and for a person: challenge someone or Lichess's AI, accept, decline (with
//! Lichess's reasons), withdraw, and list the open ones. Whether to take a challenge is the caller's decision.

use reqwest::Method;
use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::{
    Color, Error, Lichess, Side,
    client::{Body, seg},
    events::{Challenge, StatusInfo},
};

/// The clock of a game asked for.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ChallengeTime {
    /// Lichess takes an initial 0, 15, 30, 45, 60 or 90 seconds or a whole number of minutes up to 180, and an
    /// increment up to 180 seconds (60 against the AI).
    Clock {
        limit_seconds: u32,
        increment_seconds: u32,
    },
    /// Days per move: 1, 2, 3, 5, 7, 10 or 14.
    Correspondence {
        days: u32,
    },
    Unlimited,
}

impl ChallengeTime {
    fn form(self, form: &mut Vec<(&'static str, String)>) {
        match self {
            Self::Clock { limit_seconds, increment_seconds } => {
                form.push(("clock.limit", limit_seconds.to_string()));
                form.push(("clock.increment", increment_seconds.to_string()));
            }
            Self::Correspondence { days } => form.push(("days", days.to_string())),
            Self::Unlimited => {}
        }
    }
}

/// A challenge to a user.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ChallengeRequest {
    pub rated: bool,
    pub time: ChallengeTime,
    /// The colour we play.
    pub color: Color,
    /// `standard`, `chess960`, `fromPosition`, …
    pub variant: String,
    /// The start position, for `fromPosition`.
    pub fen: Option<String>,
}

impl ChallengeRequest {
    /// A standard game at this clock (seconds, seconds), random colour.
    pub fn clock(rated: bool, limit_seconds: u32, increment_seconds: u32) -> Self {
        Self {
            rated,
            time: ChallengeTime::Clock { limit_seconds, increment_seconds },
            color: Color::Random,
            variant: "standard".into(),
            fen: None,
        }
    }

    pub fn color(mut self, color: Color) -> Self {
        self.color = color;
        self
    }

    fn form(&self) -> Vec<(&'static str, String)> {
        let mut form = vec![("rated", self.rated.to_string())];
        self.time.form(&mut form);
        form.push(("color", self.color.key().to_owned()));
        form.push(("variant", self.variant.clone()));
        if let Some(fen) = &self.fen {
            form.push(("fen", fen.clone()));
        }
        form
    }
}

/// A game against Lichess's AI (Stockfish at level 1 to 8). It starts at once and is always casual.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AiChallenge {
    pub level: u8,
    pub time: ChallengeTime,
    pub color: Color,
    pub variant: String,
    pub fen: Option<String>,
}

impl AiChallenge {
    pub fn clock(level: u8, limit_seconds: u32, increment_seconds: u32) -> Self {
        Self {
            level,
            time: ChallengeTime::Clock { limit_seconds, increment_seconds },
            color: Color::Random,
            variant: "standard".into(),
            fen: None,
        }
    }

    pub fn color(mut self, color: Color) -> Self {
        self.color = color;
        self
    }
}

/// The game a challenge to the AI started.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AiGame {
    pub id: String,
    #[serde(default)]
    pub speed: Option<String>,
    #[serde(default)]
    pub rated: bool,
    #[serde(default)]
    pub fen: Option<String>,
    /// Our side.
    #[serde(default)]
    pub player: Option<Side>,
    #[serde(default)]
    pub status: Option<StatusInfo>,
    #[serde(default)]
    pub created_at: Option<i64>,
}

/// Why a challenge is declined, as Lichess names its reasons.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum DeclineReason {
    Generic,
    Later,
    TooFast,
    TooSlow,
    TimeControl,
    Rated,
    Casual,
    Standard,
    Variant,
    NoBot,
    OnlyBot,
}

impl DeclineReason {
    /// Lichess's key.
    pub fn key(self) -> &'static str {
        match self {
            Self::Generic => "generic",
            Self::Later => "later",
            Self::TooFast => "tooFast",
            Self::TooSlow => "tooSlow",
            Self::TimeControl => "timeControl",
            Self::Rated => "rated",
            Self::Casual => "casual",
            Self::Standard => "standard",
            Self::Variant => "variant",
            Self::NoBot => "noBot",
            Self::OnlyBot => "onlyBot",
        }
    }
}

/// The open challenges, to us and from us.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct ChallengeList {
    #[serde(rename = "in", default)]
    pub incoming: Vec<Challenge>,
    #[serde(rename = "out", default)]
    pub outgoing: Vec<Challenge>,
}

impl Lichess {
    /// Challenges `username`. The game, if accepted, starts on the event stream.
    pub async fn challenge_user(&self, username: &str, req: &ChallengeRequest) -> Result<Challenge, Error> {
        self.need_token()?;
        let v: Value =
            self.json(Method::POST, &format!("/api/challenge/{}", seg(username)), Body::Form(req.form())).await?;
        // The challenge, sometimes wrapped as `{ "challenge": … }`.
        let c = if v["challenge"].is_object() { v["challenge"].clone() } else { v };
        serde_json::from_value(c).map_err(|e| Error::Unexpected(format!("a challenge: {e}")))
    }

    /// Starts a game against Lichess's AI.
    pub async fn challenge_ai(&self, req: &AiChallenge) -> Result<AiGame, Error> {
        self.need_token()?;
        let mut form = vec![("level", req.level.to_string())];
        req.time.form(&mut form);
        form.push(("color", req.color.key().to_owned()));
        form.push(("variant", req.variant.clone()));
        if let Some(fen) = &req.fen {
            form.push(("fen", fen.clone()));
        }
        self.json(Method::POST, "/api/challenge/ai", Body::Form(form)).await
    }

    pub async fn accept_challenge(&self, id: &str) -> Result<(), Error> {
        self.need_token()?;
        self.post_ok(&format!("/api/challenge/{}/accept", seg(id)), Vec::new()).await
    }

    pub async fn decline_challenge(&self, id: &str, reason: DeclineReason) -> Result<(), Error> {
        self.need_token()?;
        self.post_ok(&format!("/api/challenge/{}/decline", seg(id)), vec![("reason", reason.key().to_owned())]).await
    }

    /// Withdraws one of our challenges (or aborts the game it became, if nobody has moved yet).
    pub async fn cancel_challenge(&self, id: &str) -> Result<(), Error> {
        self.need_token()?;
        self.post_ok(&format!("/api/challenge/{}/cancel", seg(id)), Vec::new()).await
    }

    /// The challenges still open, to us and from us.
    pub async fn challenges(&self) -> Result<ChallengeList, Error> {
        self.need_token()?;
        self.get("/api/challenge").await
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        events::Event,
        testing::{ChallengeAnswer, MockAccount, MockLichess, Seat},
    };

    #[tokio::test]
    async fn challenges_a_user_who_accepts_and_the_game_starts() {
        let mock = MockLichess::start().await;
        mock.add_account("tok", MockAccount::bot("RecklessBot"));
        mock.answer_challenges("alpha", ChallengeAnswer::Accept);
        let client = mock.client(Some("tok"));
        let mut events = client.events().await.unwrap();
        let sent =
            client.challenge_user("Alpha", &ChallengeRequest::clock(true, 180, 2).color(Color::White)).await.unwrap();
        assert_eq!(sent.dest_user.as_ref().unwrap().id, "alpha");
        assert_eq!(sent.time_control.limit, Some(180));
        let form = &mock.requests_to("/api/challenge/Alpha")[0].form;
        assert_eq!(form["clock.limit"], "180");
        assert_eq!(form["clock.increment"], "2");
        assert_eq!(form["color"], "white");
        assert_eq!(form["rated"], "true");
        // Our own challenge comes back on the stream, then the game.
        let Some(Event::Challenge { challenge, .. }) = events.next().await.unwrap() else { panic!() };
        assert!(challenge.is_from("recklessbot"));
        let Some(Event::GameStart { game }) = events.next().await.unwrap() else { panic!() };
        assert_eq!(game.id(), sent.id);
        assert_eq!(game.color, Some(Side::White));
    }

    #[tokio::test]
    async fn a_decline_comes_back_with_its_reason() {
        let mock = MockLichess::start().await;
        mock.add_account("tok", MockAccount::bot("RecklessBot"));
        mock.answer_challenges("alpha", ChallengeAnswer::Decline(DeclineReason::TooFast));
        let client = mock.client(Some("tok"));
        let mut events = client.events().await.unwrap();
        let sent = client.challenge_user("alpha", &ChallengeRequest::clock(false, 60, 0)).await.unwrap();
        let declined = loop {
            match events.next().await.unwrap() {
                Some(Event::ChallengeDeclined { challenge }) => break challenge,
                Some(_) => continue,
                None => panic!("the stream ended"),
            }
        };
        assert_eq!(declined.id, sent.id);
        assert_eq!(declined.decline_reason_key.as_deref(), Some("tooFast"));
    }

    #[tokio::test]
    async fn accepts_declines_lists_and_withdraws() {
        let mock = MockLichess::start().await;
        mock.add_account("tok", MockAccount::user("Dame"));
        let client = mock.client(Some("tok"));
        let one = mock.send_challenge(Seat::user("Alice", 1650), "dame", &ChallengeRequest::clock(true, 600, 5));
        let two = mock.send_challenge(Seat::user("Bob", 1500), "dame", &ChallengeRequest::clock(false, 300, 0));
        let ours = client.challenge_user("carol", &ChallengeRequest::clock(true, 600, 0)).await.unwrap();
        let open = client.challenges().await.unwrap();
        assert_eq!(open.incoming.len(), 2);
        assert_eq!(open.outgoing.len(), 1);
        assert_eq!(open.outgoing[0].id, ours.id);
        client.decline_challenge(&two, DeclineReason::Later).await.unwrap();
        assert_eq!(mock.requests_to(&format!("/api/challenge/{two}/decline"))[0].form["reason"], "later");
        client.accept_challenge(&one).await.unwrap();
        client.cancel_challenge(&ours.id).await.unwrap();
        let open = client.challenges().await.unwrap();
        assert!(open.incoming.is_empty() && open.outgoing.is_empty(), "{open:?}");
        assert_eq!(mock.ongoing_games("dame").len(), 1);
        let e = client.accept_challenge("nope").await.unwrap_err();
        assert_eq!(e.status(), Some(404));
    }

    #[tokio::test]
    async fn plays_the_ai() {
        let mock = MockLichess::start().await;
        mock.add_account("tok", MockAccount::user("Dame"));
        let client = mock.client(Some("tok"));
        let game = client.challenge_ai(&AiChallenge::clock(3, 600, 5).color(Color::Black)).await.unwrap();
        assert_eq!(game.player, Some(Side::Black));
        assert!(!game.rated);
        let form = &mock.requests_to("/api/challenge/ai")[0].form;
        assert_eq!(form["level"], "3");
        // The AI is White and moves at once.
        assert_eq!(mock.game(&game.id).unwrap().moves.len(), 1);
    }
}
