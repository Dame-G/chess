//! The Lichess clients, one per account: exactly one [`Lichess`] per token, cloned for everything that uses that
//! account, so every use shares the token's request slot as Lichess asks. And the mapping of Lichess's accounts and
//! challenges onto `core`'s.

use std::sync::RwLock;

use chess_core::{
    Color, ConnectionKey, Speed, TimeControl,
    lichess::{Account, Challenge, Direction, Opponent, Rating, Ratings, board},
};
use chess_lichess::{Config, Lichess, User, events};

pub(crate) struct Clients {
    pub config: Config,
    you: RwLock<Option<Lichess>>,
    anonymous: Lichess,
}

impl Clients {
    pub fn new(config: Config) -> Self {
        Self { anonymous: Lichess::anonymous(config.clone()), config, you: RwLock::new(None) }
    }

    fn slot(&self, key: ConnectionKey) -> &RwLock<Option<Lichess>> {
        match key {
            ConnectionKey::You => &self.you,
        }
    }

    /// The account's client, while a token is stored.
    pub fn get(&self, key: ConnectionKey) -> Option<Lichess> {
        self.slot(key).read().unwrap().clone()
    }

    /// A client for `token`, made once and kept as the account's (none forgets it).
    pub fn set(&self, key: ConnectionKey, token: Option<&str>) -> Option<Lichess> {
        let client = token.map(|t| Lichess::new(self.config.clone(), Some(t)));
        *self.slot(key).write().unwrap() = client.clone();
        client
    }

    pub fn anonymous(&self) -> Lichess {
        self.anonymous.clone()
    }

    /// Lichess's base address, for links to games.
    pub fn url(&self) -> &str {
        &self.config.base
    }
}

/// A Lichess user as a connection's account.
pub(crate) fn account_of(u: &User) -> Account {
    let rating = |perf: &str| {
        u.perfs.get(perf).and_then(|p| Some(Rating { rating: p.rating?, games: p.games, provisional: p.prov }))
    };
    Account {
        id: u.id.to_lowercase(),
        username: if u.username.is_empty() { u.id.clone() } else { u.username.clone() },
        title: u.title.clone(),
        ratings: Ratings {
            bullet: rating("bullet"),
            blitz: rating("blitz"),
            rapid: rating("rapid"),
            classical: rating("classical"),
        },
    }
}

/// A challenge from Lichess's event stream, seen from the connection whose account is `me` (a lower-case id).
pub(crate) fn challenge_of(c: &events::Challenge, connection: ConnectionKey, me: &str) -> Challenge {
    let outgoing = c.is_from(me);
    let other = if outgoing { c.dest_user.as_ref() } else { c.challenger.as_ref() };
    let tc = &c.time_control;
    let time = match tc.kind.as_str() {
        "clock" => Some(TimeControl::clock(
            u64::from(tc.limit.unwrap_or(0)) * 1000,
            u64::from(tc.increment.unwrap_or(0)) * 1000,
        )),
        "correspondence" => Some(TimeControl::Correspondence { days: tc.days_per_turn.unwrap_or(1) }),
        _ => None,
    };
    let speed = Speed::parse(&c.speed).or_else(|| time.and_then(|t| t.speed())).unwrap_or(Speed::Correspondence);
    // `color` is the challenger's choice.
    let theirs = match c.color.as_deref() {
        Some("white") => Color::White,
        Some("black") => Color::Black,
        _ => Color::Random,
    };
    let color = match (outgoing, theirs) {
        (true, c) | (false, c @ Color::Random) => c,
        (false, Color::White) => Color::Black,
        (false, Color::Black) => Color::White,
    };
    let variant = if c.variant.key.is_empty() { "standard".to_owned() } else { c.variant.key.clone() };
    Challenge {
        url: c.url.clone().unwrap_or_else(|| format!("https://lichess.org/{}", c.id)),
        id: c.id.clone(),
        connection,
        direction: if outgoing { Direction::Outgoing } else { Direction::Incoming },
        opponent: Opponent {
            name: other.map(|u| if u.name.is_empty() { u.id.clone() } else { u.name.clone() }).unwrap_or_default(),
            title: other.and_then(|u| u.title.clone()),
            rating: other.and_then(|u| u.rating),
        },
        rated: c.rated,
        playable: board::playable(speed, &variant),
        speed,
        variant,
        time,
        color,
    }
}

/// Lichess's colour for ours.
pub(crate) fn lichess_color(c: Color) -> chess_lichess::Color {
    match c {
        Color::White => chess_lichess::Color::White,
        Color::Black => chess_lichess::Color::Black,
        Color::Random => chess_lichess::Color::Random,
    }
}

/// A game clock as Lichess's challenges take it: whole seconds.
pub(crate) fn clock_seconds(time: &TimeControl) -> Result<(u32, u32), chess_core::Error> {
    match *time {
        TimeControl::Clock { initial_ms, increment_ms } => {
            Ok(((initial_ms / 1000) as u32, (increment_ms / 1000) as u32))
        }
        _ => Err(chess_core::Error::bad_request("a game on Lichess here needs a clock, such as 10+5")),
    }
}
