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
