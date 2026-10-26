//! The bot's decisions that need no network: its settings, which challenges it takes, and the clocks it challenges
//! other bots at. Plain functions over plain data, tested without Lichess.

use std::fmt;

use serde::{Deserialize, Serialize};
use utoipa::ToSchema;

use crate::time::{Speed, TimeControl};

/// The bot's settings: what plays, how, and which challenges it accepts. Stored by `play`; old or partial stored
/// settings fill in the defaults.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, ToSchema)]
#[serde(default)]
pub struct BotSettings {
    /// The version that plays: a version name such as `v14` or an id, or `champion` meaning the champion at the start
    /// of each game. Nothing plays (challenges are declined) until one is chosen.
    pub version: Option<String>,
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
    /// The speeds accepted. UltraBullet and correspondence are never played.
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

impl Default for BotSettings {
    fn default() -> Self {
        Self {
            version: None,
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

impl BotSettings {
    /// The settings with every number brought into range. Fails on what cannot be fixed by clamping.
    pub fn checked(mut self) -> Result<Self, String> {
        self.hash = self.hash.clamp(1, 4096);
        self.threads = self.threads.clamp(1, 64);
        self.move_overhead = self.move_overhead.min(2000);
        self.max_games = self.max_games.clamp(1, 16);
        self.speeds.retain(|s| !matches!(s, Speed::UltraBullet | Speed::Correspondence));
        self.speeds.sort();
        self.speeds.dedup();
        self.version = self.version.map(|b| b.trim().to_owned()).filter(|b| !b.is_empty());
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

    /// The seek clocks, read. Settings that passed [`Self::checked`] always read.
    pub fn clocks(&self) -> Vec<SeekClock> {
        self.seek_clocks.iter().filter_map(|c| SeekClock::parse(c).ok()).collect()
    }
}

/// A clock to challenge another bot at: the initial time and the increment, in seconds.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize, ToSchema)]
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
        if (seconds - seconds.round()).abs() > 1e-9 || !super::board::valid_initial(initial) {
            return Err(format!(
                "{}: Lichess clocks start at 0, 1/4, 1/2, 3/4, 1.5 or a whole number of minutes up to 180",
                text.trim()
            ));
        }
        if increment > 60 {
            return Err(format!("{}: a challenge's increment is at most 60 seconds", text.trim()));
        }
        let clock = Self { initial, increment };
        if clock.speed() == Speed::UltraBullet {
            return Err(format!("{}: that is UltraBullet, which bots cannot play", text.trim()));
        }
        Ok(clock)
    }

    /// The speed Lichess files a game at this clock under, and so the rating that counts.
    pub fn speed(self) -> Speed {
        Speed::of_clock(u64::from(self.initial) * 1000, u64::from(self.increment) * 1000)
    }

    pub fn time(self) -> TimeControl {
        TimeControl::clock(u64::from(self.initial) * 1000, u64::from(self.increment) * 1000)
    }
}

impl fmt::Display for SeekClock {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self.initial {
            15 => write!(f, "0.25+{}", self.increment),
            30 => write!(f, "0.5+{}", self.increment),
            45 => write!(f, "0.75+{}", self.increment),
            90 => write!(f, "1.5+{}", self.increment),
            s => write!(f, "{}+{}", s / 60, self.increment),
        }
    }
}

/// A challenge to the bot as its rules read it; `play` fills it from the adapter's challenge.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Offer {
    /// Lichess's variant key: `standard`, `fromPosition`, `chess960`, …
    pub variant: String,
    /// Lichess's time-control kind: `clock`, `correspondence` or `unlimited`.
    pub kind: String,
    /// Initial clock and increment, in seconds, for a clock.
    pub limit: u32,
    pub increment: u32,
    /// Lichess's speed key: `ultraBullet`, `bullet`, …
    pub speed: String,
    pub rated: bool,
    /// Whether the challenger is a BOT.
    pub from_bot: bool,
}

/// Why a challenge is declined, as Lichess's decline reasons name it.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub enum Decline {
    Generic,
    /// Busy: as many games as allowed are running (or the daily bot-game limit is reached).
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
/// accepted and about to start; `playable` whether a version is chosen and built.
pub fn decide(c: &Offer, s: &BotSettings, busy: usize, playable: bool) -> Result<(), Decline> {
    match c.variant.as_str() {
        "standard" => {}
        "fromPosition" if s.from_position => {}
        _ => return Err(Decline::Variant),
    }
    if c.kind != "clock" {
        return Err(Decline::TimeControl);
    }
    if c.limit < s.min_initial {
        return Err(Decline::TooFast);
    }
    if c.limit > s.max_initial {
        return Err(Decline::TooSlow);
    }
    if c.increment < s.min_increment || c.increment > s.max_increment {
        return Err(Decline::TimeControl);
    }
    match Speed::parse(&c.speed) {
        Some(Speed::UltraBullet) => return Err(Decline::TooFast),
        Some(speed) if s.speeds.contains(&speed) => {}
        // Anything slower than allowed reads best as the wrong clock.
        _ => return Err(Decline::TimeControl),
    }
    if c.rated && !s.accept_rated {
        return Err(Decline::Casual);
    }
    if !c.rated && !s.accept_casual {
        return Err(Decline::Rated);
    }
    if c.from_bot && !s.accept_bots {
        return Err(Decline::NoBot);
    }
    if !c.from_bot && !s.accept_humans {
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

#[cfg(test)]
mod tests {
    use super::*;

    fn challenge(speed: &str, limit: u32, increment: u32) -> Offer {
        Offer {
            variant: "standard".into(),
            kind: "clock".into(),
            limit,
            increment,
            speed: speed.into(),
            rated: true,
            from_bot: false,
        }
    }

    #[test]
    fn accepts_what_the_settings_allow() {
        let s = BotSettings::default();
        assert_eq!(decide(&challenge("blitz", 180, 2), &s, 0, true), Ok(()));
        assert_eq!(decide(&challenge("bullet", 60, 0), &s, 0, true), Ok(()));
    }

    #[test]
    fn declines_the_wrong_clock_with_the_reason_lichess_shows() {
        let s = BotSettings::default();
        let wide = BotSettings { min_initial: 0, ..BotSettings::default() };
        assert_eq!(decide(&challenge("ultraBullet", 15, 0), &s, 0, true), Err(Decline::TooFast));
        assert_eq!(decide(&challenge("ultraBullet", 15, 0), &wide, 0, true), Err(Decline::TooFast));
        assert_eq!(decide(&challenge("classical", 2700, 0), &s, 0, true), Err(Decline::TooSlow));
        // Classical within the clock limits, but not an allowed speed.
        let wide = BotSettings { max_initial: 10_800, ..BotSettings::default() };
        assert_eq!(decide(&challenge("classical", 2700, 0), &wide, 0, true), Err(Decline::TimeControl));
        assert_eq!(decide(&challenge("blitz", 180, 60), &s, 0, true), Err(Decline::TimeControl));
        let corr = Offer { kind: "correspondence".into(), limit: 0, ..challenge("correspondence", 0, 0) };
        assert_eq!(decide(&corr, &s, 0, true), Err(Decline::TimeControl));
    }

    #[test]
    fn declines_variants_unless_from_position_is_allowed() {
        let mut c = challenge("blitz", 180, 0);
        c.variant = "chess960".into();
        assert_eq!(decide(&c, &BotSettings::default(), 0, true), Err(Decline::Variant));
        c.variant = "fromPosition".into();
        assert_eq!(decide(&c, &BotSettings::default(), 0, true), Err(Decline::Variant));
        assert_eq!(decide(&c, &BotSettings { from_position: true, ..BotSettings::default() }, 0, true), Ok(()));
    }

    #[test]
    fn declines_by_mode_opponent_and_load() {
        let c = challenge("blitz", 180, 0);
        let s = BotSettings::default;
        assert_eq!(decide(&c, &BotSettings { accept_rated: false, ..s() }, 0, true), Err(Decline::Casual));
        let casual = Offer { rated: false, ..c.clone() };
        assert_eq!(decide(&casual, &BotSettings { accept_casual: false, ..s() }, 0, true), Err(Decline::Rated));
        assert_eq!(decide(&c, &BotSettings { accept_humans: false, ..s() }, 0, true), Err(Decline::OnlyBot));
        let bot = Offer { from_bot: true, ..c.clone() };
        assert_eq!(decide(&bot, &BotSettings { accept_bots: false, ..s() }, 0, true), Err(Decline::NoBot));
        assert_eq!(decide(&c, &s(), 1, true), Err(Decline::Later));
        assert_eq!(decide(&c, &BotSettings { max_games: 2, ..s() }, 1, true), Ok(()));
        assert_eq!(decide(&c, &s(), 0, false), Err(Decline::Generic));
        assert_eq!(Decline::TooFast.key(), "tooFast");
        assert_eq!(serde_json::to_value(Decline::OnlyBot).unwrap(), "onlyBot");
    }

    #[test]
    fn settings_are_brought_into_range() {
        let s = BotSettings { hash: 0, move_overhead: 9000, max_games: 0, ..BotSettings::default() }.checked().unwrap();
        assert_eq!((s.hash, s.move_overhead, s.max_games), (1, 2000, 1));
        assert!(BotSettings { min_initial: 600, max_initial: 60, ..BotSettings::default() }.checked().is_err());
        let s = BotSettings {
            speeds: vec![Speed::Rapid, Speed::UltraBullet, Speed::Bullet, Speed::Rapid],
            ..BotSettings::default()
        };
        assert_eq!(s.checked().unwrap().speeds, [Speed::Bullet, Speed::Rapid]);
        // Old or partial stored settings fill in the defaults.
        let partial: BotSettings = serde_json::from_str(r#"{ "version": "champion" }"#).unwrap();
        assert_eq!(partial.hash, 64);
        assert_eq!(partial.version.as_deref(), Some("champion"));
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
        assert_eq!(SeekClock::parse("3+2").unwrap().time(), TimeControl::clock(180_000, 2_000));
        for bad in ["3", "3+x", "0.2+0", "200+0", "3+61", "0.25+0", "0+0", "-1+2"] {
            assert!(SeekClock::parse(bad).is_err(), "{bad}");
        }
        let clocks = vec!["3+2".into(), "3+2 ".into(), "1.5+0".into()];
        let s = BotSettings { seek_clocks: clocks, seek_range: 1, ..BotSettings::default() }.checked().unwrap();
        assert_eq!(s.seek_clocks, ["3+2", "1.5+0"]);
        assert_eq!(s.seek_range, 25);
        assert_eq!(s.clocks().len(), 2);
        assert!(BotSettings { seek_clocks: vec![], ..BotSettings::default() }.checked().is_err());
        assert!(BotSettings { seek_clocks: vec!["3+90".into()], ..BotSettings::default() }.checked().is_err());
    }
}
