//! The waits the runtime chooses: how patiently it reconnects, how long it waits for an answer, when it claims a game.
//! Lichess asks for some of them (a minute after a 429); tests shorten the rest with [`Timing::fast`].

use std::time::Duration;

use dameless_core::lichess::seek;

#[derive(Clone, Debug)]
pub struct Timing {
    /// The bot's search for games: pauses between challenges, how long a challenge may go unanswered.
    pub seek: seek::Timing,
    /// A broken stream is reopened after this, doubling up to `reconnect_max`.
    pub reconnect_min: Duration,
    pub reconnect_max: Duration,
    /// After Lichess answers 429, the next try waits this long (Lichess asks for a minute).
    pub rate_limited: Duration,
    /// A claim waits this long past the moment Lichess allows it, so it does not arrive early.
    pub claim_margin: Duration,
    /// How long an action waits for its game to act on it (a move to be on the board) before answering.
    pub answer: Duration,
    /// An accepted challenge whose game has not started counts as a game for this long.
    pub accepted_for: Duration,
    /// How many times a game stream that will not open is tried before the game is given up.
    pub stream_tries: u32,
    /// The engine's time for a move in a Lichess game without a clock (correspondence).
    pub unclocked_movetime_ms: u64,
    /// The thinking engine's progress is reported at most this often.
    pub info_every: Duration,
}

impl Default for Timing {
    fn default() -> Self {
        Self {
            seek: seek::Timing::default(),
            reconnect_min: Duration::from_secs(1),
            reconnect_max: Duration::from_secs(60),
            rate_limited: Duration::from_secs(61),
            claim_margin: Duration::from_millis(500),
            answer: Duration::from_secs(10),
            accepted_for: Duration::from_secs(60),
            stream_tries: 8,
            unclocked_movetime_ms: 5_000,
            info_every: Duration::from_millis(200),
        }
    }
}

impl Timing {
    /// Every wait shortened, for tests against the mock Lichess.
    pub fn fast() -> Self {
        Self {
            seek: seek::Timing { polite: 50, answer: 1_000, rate_limited: 300, retry: 300, accepted_for: 2_000 },
            reconnect_min: Duration::from_millis(20),
            reconnect_max: Duration::from_millis(200),
            rate_limited: Duration::from_millis(300),
            claim_margin: Duration::from_millis(20),
            answer: Duration::from_secs(5),
            accepted_for: Duration::from_secs(2),
            stream_tries: 3,
            unclocked_movetime_ms: 50,
            info_every: Duration::from_millis(20),
        }
    }

    /// A backoff for streams, from these waits.
    pub(crate) fn backoff(&self) -> dameless_lichess::Backoff {
        dameless_lichess::Backoff::new(self.reconnect_min, self.reconnect_max, self.rate_limited)
    }
}
