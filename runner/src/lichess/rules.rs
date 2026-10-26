//! The bot's decisions that need no network: which challenges it takes, and how a Lichess game status reads as a
//! result and a reason. Plain functions over plain data, tested without Lichess.

use serde::{Deserialize, Serialize};
use shakmaty::Chess;
use utoipa::ToSchema;

use crate::matches::rules::{self, Ending, GameResult, Reason, Side};

/// The bot's settings: what plays, how, and which challenges it accepts. Stored in the database.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, ToSchema)]
#[serde(default)]
pub struct Settings {
    /// The build that plays: an id such as `12`, or a ref such as `main` meaning the newest build of it at the start of
    /// each game. Nothing plays (challenges are declined) until one is chosen.
    pub build: Option<String>,
    /// Hash table size in MiB. Default 64.
    pub hash: u32,
    /// Search threads per game. Default 1.
    pub threads: u32,
    /// Milliseconds the engine keeps in hand on every move for the network, as Reckless's `MoveOverhead` (0 to 2000).
    /// Default 300.
    pub move_overhead: u32,
    /// Accept rated challenges.
    pub accept_rated: bool,
    /// Accept casual challenges.
    pub accept_casual: bool,
    /// The speeds accepted. Correspondence is never played.
    pub speeds: Vec<Speed>,
    /// The shortest initial clock accepted, in seconds.
    pub min_initial: u32,
    /// The longest initial clock accepted, in seconds.
    pub max_initial: u32,
    /// The smallest increment accepted, in seconds.
    pub min_increment: u32,
    /// The largest increment accepted, in seconds.
    pub max_increment: u32,
    /// Also accept standard chess from a set position (Lichess's `fromPosition`). Other variants are always declined.
    pub from_position: bool,
    /// Accept challenges from people.
    pub accept_humans: bool,
    /// Accept challenges from other bots.
    pub accept_bots: bool,
    /// Games at once. Challenges beyond this are declined as `later`. Default 1.
    pub max_games: u32,
    /// The clocks the bot challenges other bots at when it looks for a game, as Lichess writes them: minutes plus
    /// increment seconds, such as `3+2`. One is picked at random for each challenge. Default 3+0, 3+2 and 5+3.
    pub seek_clocks: Vec<String>,
    /// Whether the challenges it sends when looking for a game are rated. Default true.
    pub seek_rated: bool,
    /// The rating window for opponents when looking for a game: bots within this many points of ours at that speed
    /// are preferred, then within twice as many, then any. Default 300.
    pub seek_range: u32,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            build: None,
            hash: 64,
            threads: 1,
            move_overhead: 300,
            accept_rated: true,
            accept_casual: true,
            speeds: vec![Speed::Bullet, Speed::Blitz, Speed::Rapid],
            min_initial: 60,
            max_initial: 1800,
            min_increment: 0,
            max_increment: 30,
            from_position: false,
            accept_humans: true,
            accept_bots: true,
            max_games: 1,
            seek_clocks: vec!["3+0".into(), "3+2".into(), "5+3".into()],
            seek_rated: true,
            seek_range: 300,
        }
    }
}

impl Settings {
    /// The settings with every number brought into range. Fails on what cannot be fixed by clamping.
    pub fn checked(mut self) -> Result<Self, String> {
        self.hash = self.hash.clamp(1, 4096);
        self.threads = self.threads.clamp(1, 64);
        self.move_overhead = self.move_overhead.min(2000);
        self.max_games = self.max_games.clamp(1, 16);
        self.speeds.sort_by_key(|s| *s as u8);
        self.speeds.dedup();
        self.build = self.build.map(|b| b.trim().to_owned()).filter(|b| !b.is_empty());
        if self.min_initial > self.max_initial {
            return Err("the shortest initial clock is longer than the longest".into());
        }
        if self.min_increment > self.max_increment {
            return Err("the smallest increment is larger than the largest".into());
        }
        let mut clocks = Vec::new();
        for text in &self.seek_clocks {
            let clock = SeekClock::parse(text)?.to_string();
            if !clocks.contains(&clock) {
                clocks.push(clock);
            }
        }
        if clocks.is_empty() {
            return Err("give at least one clock to look for games at, such as 3+2".into());
        }
        self.seek_clocks = clocks;
        self.seek_range = self.seek_range.clamp(25, 2000);
        Ok(self)
    }
}

/// A Lichess speed the bot can play. Correspondence is not one.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "lowercase")]
pub enum Speed {
    Bullet,
    Blitz,
    Rapid,
    Classical,
}

impl Speed {
    pub fn parse(key: &str) -> Option<Self> {
        match key {
            "bullet" => Some(Self::Bullet),
            "blitz" => Some(Self::Blitz),
            "rapid" => Some(Self::Rapid),
            "classical" => Some(Self::Classical),
            _ => None,
        }
    }
}

/// A clock to challenge at: the initial time and the increment, in seconds.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, ToSchema)]
pub struct SeekClock {
    /// Seconds.
    pub initial: u32,
    /// Seconds.
    pub increment: u32,
}

impl SeekClock {
    /// Reads `minutes+seconds` as Lichess writes a clock (`3+2`, `0.5+0`), within what a challenge to a bot can have:
    /// an initial clock of 0, 15, 30, 45 or 90 seconds or whole minutes up to 180, an increment up to 60 seconds, and
    /// no UltraBullet (which bots do not play).
    pub fn parse(text: &str) -> Result<Self, String> {
        let bad = || format!("a clock looks like 3+2 (minutes plus increment seconds), not {text:?}");
        let (minutes, increment) = text.trim().split_once('+').ok_or_else(bad)?;
        let minutes: f64 = minutes.trim().parse().map_err(|_| bad())?;
        let increment: u32 = increment.trim().parse().map_err(|_| bad())?;
        if !minutes.is_finite() || minutes < 0.0 {
            return Err(bad());
        }
        let seconds = minutes * 60.0;
        let initial = seconds.round() as u32;
        if (seconds - seconds.round()).abs() > 1e-9
            || !(matches!(initial, 0 | 15 | 30 | 45 | 90) || (initial.is_multiple_of(60) && initial <= 10_800))
        {
            return Err(format!(
                "{}: Lichess clocks start at 0, 1/4, 1/2, 3/4, 1.5 or a whole number of minutes up to 180",
                text.trim()
            ));
        }
        if increment > 60 {
            return Err(format!("{}: a challenge's increment is at most 60 seconds", text.trim()));
        }
        let clock = Self { initial, increment };
        if clock.estimate() < 30 {
            return Err(format!("{}: that is UltraBullet, which bots cannot play", text.trim()));
        }
        Ok(clock)
    }

    /// Lichess's estimate of a game's length, which sets its speed: the initial clock plus 40 increments.
    fn estimate(self) -> u32 {
        self.initial + 40 * self.increment
    }

    /// The speed Lichess files a game at this clock under, and so the rating that counts.
    pub fn speed(self) -> Speed {
        match self.estimate() {
            ..180 => Speed::Bullet,
            180..480 => Speed::Blitz,
            480..1500 => Speed::Rapid,
            _ => Speed::Classical,
        }
    }
}

impl std::fmt::Display for SeekClock {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self.initial {
            15 => write!(f, "0.25+{}", self.increment),
            30 => write!(f, "0.5+{}", self.increment),
            45 => write!(f, "0.75+{}", self.increment),
            90 => write!(f, "1.5+{}", self.increment),
            s => write!(f, "{}+{}", s / 60, self.increment),
        }
    }
}

// ---------------------------------------------------------------------------------------------------------------------
// Challenges
// ---------------------------------------------------------------------------------------------------------------------

/// A Lichess user as challenges and games name them.
#[derive(Clone, Debug, Default, PartialEq, Deserialize)]
pub struct LichessUser {
    #[serde(default)]
    pub id: String,
    #[serde(default)]
    pub name: String,
    pub title: Option<String>,
    pub rating: Option<u32>,
}

#[derive(Clone, Debug, Default, PartialEq, Deserialize)]
pub struct Variant {
    pub key: String,
}

#[derive(Clone, Debug, Default, PartialEq, Deserialize)]
pub struct TimeControl {
    /// `clock`, `correspondence` or `unlimited`.
    #[serde(rename = "type")]
    pub kind: String,
    /// Seconds.
    pub limit: Option<u32>,
    /// Seconds.
    pub increment: Option<u32>,
}

/// A challenge as the event stream delivers it.
#[derive(Clone, Debug, Default, PartialEq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Challenge {
    pub id: String,
    #[serde(default)]
    pub rated: bool,
    #[serde(default)]
    pub speed: String,
    #[serde(default)]
    pub variant: Variant,
    #[serde(default)]
    pub time_control: TimeControl,
    pub challenger: Option<LichessUser>,
    pub dest_user: Option<LichessUser>,
}

/// Why a challenge is declined, as Lichess's decline reasons name it.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub enum Decline {
    Generic,
    /// Busy: as many games as allowed are running.
    Later,
    TooFast,
    TooSlow,
    TimeControl,
    /// Only rated games are accepted.
    Rated,
    /// Only casual games are accepted.
    Casual,
    Variant,
    /// Bots are not accepted.
    NoBot,
    /// Only bots are accepted.
    OnlyBot,
}

impl Decline {
    /// The key Lichess takes in `reason`.
    pub fn key(self) -> &'static str {
        match self {
            Self::Generic => "generic",
            Self::Later => "later",
            Self::TooFast => "tooFast",
            Self::TooSlow => "tooSlow",
            Self::TimeControl => "timeControl",
            Self::Rated => "rated",
            Self::Casual => "casual",
            Self::Variant => "variant",
            Self::NoBot => "noBot",
            Self::OnlyBot => "onlyBot",
        }
    }
}

/// Whether to take a challenge: `Ok` to accept, else the reason to decline. `busy` is how many games are running or
/// accepted and about to start; `playable` whether a build is chosen and exists.
pub fn decide(c: &Challenge, s: &Settings, busy: usize, playable: bool) -> Result<(), Decline> {
    match c.variant.key.as_str() {
        "standard" => {}
        "fromPosition" if s.from_position => {}
        _ => return Err(Decline::Variant),
    }
    if c.time_control.kind != "clock" {
        return Err(Decline::TimeControl);
    }
    let (limit, increment) = (c.time_control.limit.unwrap_or(0), c.time_control.increment.unwrap_or(0));
    if limit < s.min_initial {
        return Err(Decline::TooFast);
    }
    if limit > s.max_initial {
        return Err(Decline::TooSlow);
    }
    if increment < s.min_increment || increment > s.max_increment {
        return Err(Decline::TimeControl);
    }
    match Speed::parse(&c.speed) {
        Some(speed) if s.speeds.contains(&speed) => {}
        // Ultrabullet and anything slower than allowed reads best as too fast or slow; the rest as the wrong clock.
        _ if c.speed == "ultraBullet" => return Err(Decline::TooFast),
        _ => return Err(Decline::TimeControl),
    }
    if c.rated && !s.accept_rated {
        return Err(Decline::Casual);
    }
    if !c.rated && !s.accept_casual {
        return Err(Decline::Rated);
    }
    let bot = c.challenger.as_ref().and_then(|u| u.title.as_deref()) == Some("BOT");
    if bot && !s.accept_bots {
        return Err(Decline::NoBot);
    }
    if !bot && !s.accept_humans {
        return Err(Decline::OnlyBot);
    }
    if !playable {
        return Err(Decline::Generic);
    }
    if busy >= s.max_games as usize {
        return Err(Decline::Later);
    }
    Ok(())
}

// ---------------------------------------------------------------------------------------------------------------------
// The end of a game
// ---------------------------------------------------------------------------------------------------------------------

/// How a Lichess game status ends the record, or `None` while it is still being played. `winner` is Lichess's; `pos`
/// and `history` (hashes of every position, the current last) tell a drawn game's reason apart.
pub fn ending(status: &str, winner: Option<Side>, pos: &Chess, history: &[u64]) -> Option<Ending> {
    let reason = match status {
        "created" | "started" => return None,
        "aborted" => return Some(Ending { result: GameResult::Unfinished, reason: Reason::Aborted }),
        "mate" => Reason::Checkmate,
        "resign" => Reason::Resigned,
        "stalemate" => Reason::Stalemate,
        "timeout" => Reason::Abandoned,
        "outoftime" => Reason::Time,
        "cheat" => Reason::Cheat,
        "noStart" => Reason::NoStart,
        "variantEnd" => Reason::VariantEnd,
        // A win on time claimed against a side that could not have mated: a draw.
        "insufficientMaterialClaim" => Reason::InsufficientMaterial,
        // A draw by agreement, or claimed: the position says which rule, if any, was claimed.
        "draw" => match rules::detect(pos, history) {
            Some(e) if e.result == GameResult::Draw => e.reason,
            _ => Reason::Agreement,
        },
        _ => Reason::Other,
    };
    let result = winner.map_or(GameResult::Draw, GameResult::won_by);
    Some(Ending { result, reason })
}

#[cfg(test)]
mod tests {
    use shakmaty::{CastlingMode, fen::Fen};

    use super::*;

    fn challenge(speed: &str, limit: u32, increment: u32) -> Challenge {
        Challenge {
            id: "c1".into(),
            rated: true,
            speed: speed.into(),
            variant: Variant { key: "standard".into() },
            time_control: TimeControl { kind: "clock".into(), limit: Some(limit), increment: Some(increment) },
            challenger: Some(LichessUser { id: "someone".into(), name: "Someone".into(), title: None, rating: None }),
            dest_user: None,
        }
    }

    #[test]
    fn accepts_what_the_settings_allow() {
        let s = Settings::default();
        assert_eq!(decide(&challenge("blitz", 180, 2), &s, 0, true), Ok(()));
        assert_eq!(decide(&challenge("bullet", 60, 0), &s, 0, true), Ok(()));
    }

    #[test]
    fn declines_the_wrong_clock_with_the_reason_lichess_shows() {
        let s = Settings::default();
        assert_eq!(decide(&challenge("ultraBullet", 15, 0), &s, 0, true), Err(Decline::TooFast));
        assert_eq!(decide(&challenge("classical", 2700, 0), &s, 0, true), Err(Decline::TooSlow));
        // Classical within the clock limits, but not an allowed speed.
        let wide = Settings { max_initial: 10_800, ..Settings::default() };
        assert_eq!(decide(&challenge("classical", 2700, 0), &wide, 0, true), Err(Decline::TimeControl));
        assert_eq!(decide(&challenge("blitz", 180, 60), &s, 0, true), Err(Decline::TimeControl));
        let mut corr = challenge("correspondence", 0, 0);
        corr.time_control = TimeControl { kind: "correspondence".into(), limit: None, increment: None };
        assert_eq!(decide(&corr, &s, 0, true), Err(Decline::TimeControl));
    }

    #[test]
    fn declines_variants_unless_from_position_is_allowed() {
        let mut c = challenge("blitz", 180, 0);
        c.variant.key = "chess960".into();
        assert_eq!(decide(&c, &Settings::default(), 0, true), Err(Decline::Variant));
        c.variant.key = "fromPosition".into();
        assert_eq!(decide(&c, &Settings::default(), 0, true), Err(Decline::Variant));
        assert_eq!(decide(&c, &Settings { from_position: true, ..Settings::default() }, 0, true), Ok(()));
    }

    #[test]
    fn declines_by_mode_opponent_and_load() {
        let c = challenge("blitz", 180, 0);
        assert_eq!(decide(&c, &Settings { accept_rated: false, ..Settings::default() }, 0, true), Err(Decline::Casual));
        let mut casual = c.clone();
        casual.rated = false;
        assert_eq!(
            decide(&casual, &Settings { accept_casual: false, ..Settings::default() }, 0, true),
            Err(Decline::Rated)
        );
        assert_eq!(
            decide(&c, &Settings { accept_humans: false, ..Settings::default() }, 0, true),
            Err(Decline::OnlyBot)
        );
        let mut bot = c.clone();
        bot.challenger.as_mut().unwrap().title = Some("BOT".into());
        assert_eq!(decide(&bot, &Settings { accept_bots: false, ..Settings::default() }, 0, true), Err(Decline::NoBot));
        assert_eq!(decide(&c, &Settings::default(), 1, true), Err(Decline::Later));
        assert_eq!(decide(&c, &Settings { max_games: 2, ..Settings::default() }, 1, true), Ok(()));
        assert_eq!(decide(&c, &Settings::default(), 0, false), Err(Decline::Generic));
    }

    #[test]
    fn settings_are_brought_into_range() {
        let s = Settings { hash: 0, move_overhead: 9000, max_games: 0, ..Settings::default() }.checked().unwrap();
        assert_eq!((s.hash, s.move_overhead, s.max_games), (1, 2000, 1));
        assert!(Settings { min_initial: 600, max_initial: 60, ..Settings::default() }.checked().is_err());
        // Old or partial stored settings fill in the defaults.
        let partial: Settings = serde_json::from_str(r#"{ "build": "main" }"#).unwrap();
        assert_eq!(partial.hash, 64);
        assert_eq!(partial.build.as_deref(), Some("main"));
        assert_eq!(partial.seek_clocks, ["3+0", "3+2", "5+3"]);
        assert!(partial.seek_rated);
    }

    #[test]
    fn seek_clocks_are_read_as_lichess_writes_them() {
        assert_eq!(SeekClock::parse("3+2"), Ok(SeekClock { initial: 180, increment: 2 }));
        assert_eq!(SeekClock::parse(" 0.5+1 ").map(|c| c.to_string()), Ok("0.5+1".into()));
        assert_eq!(SeekClock::parse("1+0").unwrap().speed(), Speed::Bullet);
        assert_eq!(SeekClock::parse("3+0").unwrap().speed(), Speed::Blitz);
        assert_eq!(SeekClock::parse("10+0").unwrap().speed(), Speed::Rapid);
        assert_eq!(SeekClock::parse("30+20").unwrap().speed(), Speed::Classical);
        for bad in ["3", "3+x", "0.2+0", "200+0", "3+61", "0.25+0", "0+0", "-1+2"] {
            assert!(SeekClock::parse(bad).is_err(), "{bad}");
        }
        let clocks = vec!["3+2".into(), "3+2 ".into(), "1.5+0".into()];
        let s = Settings { seek_clocks: clocks, seek_range: 1, ..Settings::default() }.checked().unwrap();
        assert_eq!(s.seek_clocks, ["3+2", "1.5+0"]);
        assert_eq!(s.seek_range, 25);
        assert!(Settings { seek_clocks: vec![], ..Settings::default() }.checked().is_err());
        assert!(Settings { seek_clocks: vec!["3+90".into()], ..Settings::default() }.checked().is_err());
    }

    fn pos(fen: &str) -> Chess {
        fen.parse::<Fen>().unwrap().into_position(CastlingMode::Standard).unwrap()
    }

    #[test]
    fn maps_every_lichess_status() {
        let start = Chess::default();
        let h = [rules::hash(&start)];
        let end = |status, winner| ending(status, winner, &start, &h);
        assert_eq!(end("started", None), None);
        assert_eq!(end("created", None), None);
        assert_eq!(end("aborted", None), Some(Ending { result: GameResult::Unfinished, reason: Reason::Aborted }));
        assert_eq!(
            end("mate", Some(Side::Black)),
            Some(Ending { result: GameResult::Black, reason: Reason::Checkmate })
        );
        assert_eq!(
            end("resign", Some(Side::White)),
            Some(Ending { result: GameResult::White, reason: Reason::Resigned })
        );
        assert_eq!(end("stalemate", None), Some(Ending { result: GameResult::Draw, reason: Reason::Stalemate }));
        assert_eq!(end("timeout", Some(Side::White)).unwrap().reason, Reason::Abandoned);
        // A player who left can also be claimed as a draw.
        assert_eq!(end("timeout", None).unwrap().result, GameResult::Draw);
        assert_eq!(
            end("outoftime", Some(Side::Black)),
            Some(Ending { result: GameResult::Black, reason: Reason::Time })
        );
        // Out of time against bare kings is a draw.
        assert_eq!(end("outoftime", None).unwrap().result, GameResult::Draw);
        assert_eq!(end("cheat", Some(Side::White)).unwrap().reason, Reason::Cheat);
        assert_eq!(end("noStart", Some(Side::Black)).unwrap().reason, Reason::NoStart);
        assert_eq!(end("variantEnd", Some(Side::White)).unwrap().reason, Reason::VariantEnd);
        assert_eq!(end("unknownFinish", None).unwrap().reason, Reason::Other);
        assert_eq!(
            end("insufficientMaterialClaim", None),
            Some(Ending { result: GameResult::Draw, reason: Reason::InsufficientMaterial })
        );
        assert_eq!(end("draw", None), Some(Ending { result: GameResult::Draw, reason: Reason::Agreement }));
    }

    #[test]
    fn a_claimed_draw_names_the_rule() {
        let bare = pos("8/8/4k3/8/8/3K4/8/8 w - - 0 1");
        assert_eq!(ending("draw", None, &bare, &[rules::hash(&bare)]).unwrap().reason, Reason::InsufficientMaterial);
        let fifty = pos("4k3/8/8/8/8/8/8/R3K3 w - - 100 80");
        assert_eq!(ending("draw", None, &fifty, &[rules::hash(&fifty)]).unwrap().reason, Reason::FiftyMoves);
    }
}
