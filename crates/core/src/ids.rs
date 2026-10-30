//! Identity: typed ids, the two sides of a board, who sits at it, which Lichess connection, and timestamps.
//!
//! Ids are plain database row ids wrapped in their own types, so a game id can never be passed where a work id is
//! wanted. They serialize as bare numbers.

use std::{borrow::Cow, fmt, str::FromStr};

use serde::{Deserialize, Deserializer, Serialize, Serializer};
use utoipa::{
    PartialSchema, ToSchema,
    openapi::{RefOr, Schema, schema::ObjectBuilder, schema::Type},
};

macro_rules! id {
    ($(#[$doc:meta])* $name:ident) => {
        $(#[$doc])*
        #[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize, ToSchema)]
        #[serde(transparent)]
        pub struct $name(pub i64);

        impl fmt::Display for $name {
            fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                self.0.fmt(f)
            }
        }

        impl FromStr for $name {
            type Err = std::num::ParseIntError;

            fn from_str(s: &str) -> Result<Self, Self::Err> {
                s.trim().parse().map(Self)
            }
        }

        impl From<i64> for $name {
            fn from(id: i64) -> Self {
                Self(id)
            }
        }
    };
}

id!(
    /// A game: played here, on Lichess, or imported.
    GameId
);
id!(
    /// Anything running in the background.
    WorkId
);
id!(
    /// A review of a game.
    ReviewId
);

// ---------------------------------------------------------------------------------------------------------------------
// Sides
// ---------------------------------------------------------------------------------------------------------------------

/// One side of the board.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum Side {
    White,
    Black,
}

impl Side {
    pub const BOTH: [Self; 2] = [Self::White, Self::Black];

    /// 0 for White, 1 for Black, for arrays indexed by side.
    pub fn index(self) -> usize {
        self as usize
    }

    pub fn other(self) -> Self {
        match self {
            Self::White => Self::Black,
            Self::Black => Self::White,
        }
    }

    /// The word as PGN and people write it.
    pub fn name(self) -> &'static str {
        match self {
            Self::White => "White",
            Self::Black => "Black",
        }
    }
}

impl From<shakmaty::Color> for Side {
    fn from(c: shakmaty::Color) -> Self {
        match c {
            shakmaty::Color::White => Self::White,
            shakmaty::Color::Black => Self::Black,
        }
    }
}

impl From<Side> for shakmaty::Color {
    fn from(s: Side) -> Self {
        match s {
            Side::White => Self::White,
            Side::Black => Self::Black,
        }
    }
}

/// A colour chosen when starting a game or a challenge: a side, or let chance decide.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum Color {
    White,
    Black,
    #[default]
    Random,
}

impl Color {
    /// The side this colour gives, with `coin` deciding a random one (true is White). Randomness is passed in, as
    /// nothing in `core` draws it.
    pub fn resolve(self, coin: bool) -> Side {
        match self {
            Self::White => Side::White,
            Self::Black => Side::Black,
            Self::Random if coin => Side::White,
            Self::Random => Side::Black,
        }
    }
}

/// One value per side, such as both clocks or both accuracies.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, Serialize, Deserialize, ToSchema)]
pub struct SidePair<T> {
    pub white: T,
    pub black: T,
}

impl<T> SidePair<T> {
    pub fn new(white: T, black: T) -> Self {
        Self { white, black }
    }

    pub fn get(&self, side: Side) -> &T {
        match side {
            Side::White => &self.white,
            Side::Black => &self.black,
        }
    }

    pub fn get_mut(&mut self, side: Side) -> &mut T {
        match side {
            Side::White => &mut self.white,
            Side::Black => &mut self.black,
        }
    }

    pub fn map<U>(self, mut f: impl FnMut(T) -> U) -> SidePair<U> {
        SidePair { white: f(self.white), black: f(self.black) }
    }
}

impl<T: Clone> SidePair<T> {
    /// The same value for both sides.
    pub fn both(value: T) -> Self {
        Self { white: value.clone(), black: value }
    }
}

// ---------------------------------------------------------------------------------------------------------------------
// Players and connections
// ---------------------------------------------------------------------------------------------------------------------

/// Who sits at a board. Serialized as a string key: `you`, `engine:<id>` (an engine of the engines list, by its id)
/// and `lichess:<id>` (Lichess's lower-case user id).
#[derive(Clone, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum PlayerKey {
    You,
    Engine(String),
    Lichess(String),
}

/// Why a player key does not read.
#[derive(Clone, Debug, PartialEq, Eq, thiserror::Error)]
#[error("{0:?} is not a player: use you, engine:<id> or lichess:<username>")]
pub struct BadPlayerKey(pub String);

impl PlayerKey {
    /// A Lichess player by username, as the key stores it: Lichess ids are the username in lower case.
    pub fn lichess(username: &str) -> Result<Self, BadPlayerKey> {
        let id = username.trim().to_lowercase();
        if valid_lichess_name(&id) { Ok(Self::Lichess(id)) } else { Err(BadPlayerKey(username.to_owned())) }
    }

    /// An engine by its id, which must be a slug ([`valid_engine_id`]).
    pub fn engine(id: &str) -> Result<Self, BadPlayerKey> {
        let id = id.trim();
        if valid_engine_id(id) { Ok(Self::Engine(id.to_owned())) } else { Err(BadPlayerKey(id.to_owned())) }
    }

    /// The engine's id for an engine player.
    pub fn engine_id(&self) -> Option<&str> {
        match self {
            Self::Engine(id) => Some(id),
            _ => None,
        }
    }

    /// The Lichess username (lower case) for a Lichess player.
    pub fn lichess_id(&self) -> Option<&str> {
        match self {
            Self::Lichess(id) => Some(id),
            _ => None,
        }
    }
}

/// An engine's id: 1 to 40 lower-case letters, digits and `-`, starting with a letter or digit (`stockfish`,
/// `my-engine-2`).
pub fn valid_engine_id(id: &str) -> bool {
    !id.is_empty()
        && id.len() <= 40
        && !id.starts_with('-')
        && id.chars().all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-')
}

/// A Lichess username: 1 to 30 letters, digits, `_` and `-`.
pub fn valid_lichess_name(name: &str) -> bool {
    !name.is_empty() && name.len() <= 30 && name.chars().all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '-')
}

impl fmt::Display for PlayerKey {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::You => f.write_str("you"),
            Self::Engine(id) => write!(f, "engine:{id}"),
            Self::Lichess(id) => write!(f, "lichess:{id}"),
        }
    }
}

impl FromStr for PlayerKey {
    type Err = BadPlayerKey;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let bad = || BadPlayerKey(s.to_owned());
        match s.trim() {
            "you" => Ok(Self::You),
            other => match other.split_once(':') {
                Some(("lichess", name)) => Self::lichess(name).map_err(|_| bad()),
                Some(("engine", id)) => Self::engine(id).map_err(|_| bad()),
                _ => Err(bad()),
            },
        }
    }
}

impl Serialize for PlayerKey {
    fn serialize<S: Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        s.collect_str(self)
    }
}

impl<'de> Deserialize<'de> for PlayerKey {
    fn deserialize<D: Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        let s = Cow::<str>::deserialize(d)?;
        s.parse().map_err(serde::de::Error::custom)
    }
}

impl PartialSchema for PlayerKey {
    fn schema() -> RefOr<Schema> {
        ObjectBuilder::new()
            .schema_type(Type::String)
            .description(Some("`you`, `engine:<id>` or `lichess:<id>`"))
            .examples(["you", "engine:stockfish", "lichess:cutemouse83"])
            .into()
    }
}

impl ToSchema for PlayerKey {}

/// A Lichess account linked to the app: yours, through the Board API. It is the only one, and stays an enum so games
/// and routes name it the same way everywhere.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum ConnectionKey {
    You,
}

impl ConnectionKey {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::You => "you",
        }
    }

    /// The player this connection plays as.
    pub fn player(self) -> PlayerKey {
        match self {
            Self::You => PlayerKey::You,
        }
    }
}

impl fmt::Display for ConnectionKey {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

impl FromStr for ConnectionKey {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.trim() {
            "you" => Ok(Self::You),
            other => Err(format!("{other:?} is not a connection: use you")),
        }
    }
}

// ---------------------------------------------------------------------------------------------------------------------
// Time stamps
// ---------------------------------------------------------------------------------------------------------------------

/// A moment as an ISO-8601 UTC string, always written `2026-10-25T14:32:07.123Z` so they sort as text. `core` never
/// reads a clock: whoever makes one passes the time in (milliseconds since the epoch, or a string from Lichess or the
/// database).
#[derive(Clone, Debug, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize, ToSchema)]
#[serde(transparent)]
pub struct Timestamp(pub String);

impl Timestamp {
    /// The moment `ms` milliseconds after the Unix epoch.
    pub fn from_unix_ms(ms: i64) -> Self {
        let secs = ms.div_euclid(1000);
        let (y, m, d) = civil_from_days(secs.div_euclid(86_400));
        let s = secs.rem_euclid(86_400);
        Self(format!(
            "{y:04}-{m:02}-{d:02}T{:02}:{:02}:{:02}.{:03}Z",
            s / 3600,
            s % 3600 / 60,
            s % 60,
            ms.rem_euclid(1000)
        ))
    }

    /// Milliseconds since the epoch, when the string reads as a UTC time.
    pub fn unix_ms(&self) -> Option<i64> {
        parse_iso_ms(&self.0)
    }

    /// Reads a UTC time as Lichess, SQLite or people write it and stores it in the one form.
    pub fn parse(s: &str) -> Option<Self> {
        parse_iso_ms(s).map(Self::from_unix_ms)
    }

    /// The date part, `2026-10-25`.
    pub fn date(&self) -> &str {
        self.0.get(..10).unwrap_or(&self.0)
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl fmt::Display for Timestamp {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

/// Days since 1970-01-01 of a civil date (Howard Hinnant's algorithm).
fn days_from_civil(y: i64, m: i64, d: i64) -> i64 {
    let y = if m <= 2 { y - 1 } else { y };
    let era = y.div_euclid(400);
    let yoe = y - era * 400;
    let mp = (m + 9) % 12;
    let doy = (153 * mp + 2) / 5 + d - 1;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    era * 146_097 + doe - 719_468
}

fn civil_from_days(z: i64) -> (i64, i64, i64) {
    let z = z + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z - era * 146_097;
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    (if m <= 2 { yoe + era * 400 + 1 } else { yoe + era * 400 }, m, d)
}

/// A UTC time as Lichess, SQLite or people write it (`2026-10-25T14:32:00Z`, `2026-10-25T14:32:00.123Z`,
/// `2026-10-25 14:32:00`, `2026-10-25`), in milliseconds since the epoch. A time without a zone is read as UTC.
pub fn parse_iso_ms(s: &str) -> Option<i64> {
    let s = s.trim().trim_end_matches(['.', ',']);
    let s = s.strip_suffix('Z').or_else(|| s.strip_suffix("+00:00")).unwrap_or(s);
    let (date, time) = s.split_once(['T', ' ']).unwrap_or((s, "00:00:00"));
    let mut date = date.split('-').map(str::parse::<i64>);
    let (y, m, d) = (date.next()?.ok()?, date.next()?.ok()?, date.next()?.ok()?);
    let (time, frac) = time.split_once('.').unwrap_or((time, ""));
    let mut time = time.split(':').map(str::parse::<i64>);
    let (h, mi, sec) = (time.next()?.ok()?, time.next().unwrap_or(Ok(0)).ok()?, time.next().unwrap_or(Ok(0)).ok()?);
    if !(1..=12).contains(&m) || !(1..=31).contains(&d) || h > 23 || mi > 59 || sec > 60 {
        return None;
    }
    if !frac.chars().all(|c| c.is_ascii_digit()) {
        return None;
    }
    let ms: i64 = format!("{frac:0<3}").get(..3).and_then(|f| f.parse().ok()).unwrap_or(0);
    Some(((days_from_civil(y, m, d) * 86_400) + h * 3600 + mi * 60 + sec) * 1000 + ms)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn player_keys_read_and_write_as_strings() {
        for (text, key) in [
            ("you", PlayerKey::You),
            ("engine:stockfish", PlayerKey::Engine("stockfish".into())),
            ("engine:my-engine-2", PlayerKey::Engine("my-engine-2".into())),
            ("lichess:cutemouse83", PlayerKey::Lichess("cutemouse83".into())),
        ] {
            assert_eq!(text.parse::<PlayerKey>().unwrap(), key);
            assert_eq!(key.to_string(), text);
            assert_eq!(serde_json::to_value(&key).unwrap(), serde_json::json!(text));
            assert_eq!(serde_json::from_value::<PlayerKey>(serde_json::json!(text)).unwrap(), key);
        }
        // Lichess ids are lower case.
        assert_eq!("lichess:CuteMouse83".parse::<PlayerKey>().unwrap(), PlayerKey::Lichess("cutemouse83".into()));
        for bad in
            ["", "me", "lichess:", "lichess:a b", "engine:", "engine:Big", "engine:-x", "dameless", "stockfish:5"]
        {
            assert!(bad.parse::<PlayerKey>().is_err(), "{bad}");
        }
        assert!(serde_json::from_value::<PlayerKey>(serde_json::json!("nobody")).is_err());
    }

    #[test]
    fn ids_are_bare_numbers() {
        assert_eq!(serde_json::to_value(GameId(7)).unwrap(), serde_json::json!(7));
        assert_eq!("12".parse::<GameId>().unwrap(), GameId(12));
        assert_eq!(serde_json::to_value(SidePair::new(1, 2)).unwrap(), serde_json::json!({ "white": 1, "black": 2 }));
    }

    #[test]
    fn sides_and_colours() {
        assert_eq!(Side::White.other(), Side::Black);
        assert_eq!(serde_json::to_value(Side::Black).unwrap(), serde_json::json!("black"));
        assert_eq!(Color::Random.resolve(true), Side::White);
        assert_eq!(Color::Black.resolve(true), Side::Black);
        assert!("bot".parse::<ConnectionKey>().is_err());
        assert_eq!(ConnectionKey::You.player(), PlayerKey::You);
    }

    #[test]
    fn timestamps_go_both_ways() {
        let t = parse_iso_ms("2026-10-25T14:32:07Z").unwrap();
        assert_eq!(Timestamp::from_unix_ms(t).as_str(), "2026-10-25T14:32:07.000Z");
        assert_eq!(parse_iso_ms("1970-01-01T00:00:00Z"), Some(0));
        assert_eq!(parse_iso_ms("2000-03-01T00:00:00.250Z"), Some(951_868_800_250));
        assert_eq!(parse_iso_ms("2026-10-25T14:32:07.5Z."), Some(t + 500));
        assert_eq!(parse_iso_ms("2026-10-25 14:32:07"), Some(t));
        assert_eq!(parse_iso_ms("2026-10-25"), Some(t - (14 * 3600 + 32 * 60 + 7) * 1000));
        assert_eq!(parse_iso_ms("tomorrow"), None);
        let ts = Timestamp::parse("2026-01-02T03:04:05.6Z").unwrap();
        assert_eq!(ts.as_str(), "2026-01-02T03:04:05.600Z");
        assert_eq!(ts.date(), "2026-01-02");
        assert_eq!(Timestamp::from_unix_ms(-1).as_str(), "1969-12-31T23:59:59.999Z");
    }
}
