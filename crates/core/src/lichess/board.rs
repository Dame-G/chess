//! What Lichess's Board API lets your own account play (its documentation and lila's `isBoardCompatible`): rapid,
//! classical and correspondence; blitz too for direct challenges and games against the AI; never bullet. A game's
//! speed is Lichess's estimate, initial + 40 × increment (see [`Speed::of_clock`]).
//!
//! So a seek must be at least 8 minutes estimated, a challenge at least 3, and a challenge's clock is 0, 15, 30, 45 or
//! 90 seconds or whole minutes up to 180, with an increment of at most 180 seconds (60 against the AI).

use crate::{
    error::Error,
    game::{Offers, SeatAction},
    ids::Side,
    time::Speed,
};

/// The longest increment of a challenge, in seconds.
pub const MAX_INCREMENT: u32 = 180;
/// The longest increment of a game against Lichess's AI, in seconds.
pub const MAX_AI_INCREMENT: u32 = 60;
/// The longest initial clock, in seconds: 180 minutes.
pub const MAX_INITIAL: u32 = 10_800;
/// The shortest estimated game a seek may be, in seconds: rapid.
pub const MIN_SEEK_ESTIMATE: u32 = 480;
/// The shortest estimated game a challenge may be, in seconds: blitz.
pub const MIN_CHALLENGE_ESTIMATE: u32 = 180;

/// Lichess's estimate of a game's length in seconds, which sets its speed.
pub fn estimate(initial: u32, increment: u32) -> u32 {
    initial + 40 * increment
}

/// An initial clock Lichess allows in a challenge: 0, 15, 30, 45 or 90 seconds, or whole minutes up to 180.
pub fn valid_initial(seconds: u32) -> bool {
    matches!(seconds, 0 | 15 | 30 | 45 | 90) || (seconds.is_multiple_of(60) && seconds <= MAX_INITIAL)
}

/// Whether a game at this speed can be played through the Board API at all.
pub fn playable_speed(speed: Speed) -> bool {
    matches!(speed, Speed::Blitz | Speed::Rapid | Speed::Classical | Speed::Correspondence)
}

/// Whether a challenge can be played here: a playable speed and standard chess (or from a position).
pub fn playable(speed: Speed, variant: &str) -> bool {
    playable_speed(speed) && matches!(variant, "standard" | "fromPosition")
}

/// Checks a challenge's clock (`ai` for a game against Lichess's AI, which allows less increment).
pub fn check_challenge(initial: u32, increment: u32, ai: bool) -> Result<(), Error> {
    if !valid_initial(initial) {
        return Err(Error::bad_request(
            "Lichess clocks start at 0, 15, 30, 45 or 90 seconds, or a whole number of minutes up to 180",
        ));
    }
    let max = if ai { MAX_AI_INCREMENT } else { MAX_INCREMENT };
    if increment > max {
        return Err(Error::bad_request(format!("the increment is at most {max} seconds here")));
    }
    if estimate(initial, increment) < MIN_CHALLENGE_ESTIMATE {
        return Err(Error::bad_request(
            "bullet cannot be played through Lichess's Board API: choose blitz or slower (initial + 40 × increment \
             at least 3 minutes), or play bullet on lichess.org",
        ));
    }
    Ok(())
}

/// Checks a seek's clock: at most 180 minutes plus 180 seconds, and rapid or slower.
pub fn check_seek(initial: u32, increment: u32) -> Result<(), Error> {
    if initial > MAX_INITIAL || increment > MAX_INCREMENT {
        return Err(Error::bad_request("a seek's clock is at most 180 minutes plus 180 seconds"));
    }
    if estimate(initial, increment) < MIN_SEEK_ESTIMATE {
        return Err(Error::bad_request(
            "Lichess's Board API seeks only rapid and classical games (initial + 40 × increment at least 8 minutes): \
             for blitz, challenge someone directly",
        ));
    }
    Ok(())
}

/// Reads a seek's rating range, `1500-1800`; blank is none.
pub fn rating_range(text: Option<&str>) -> Result<Option<String>, Error> {
    let Some(r) = text.map(str::trim).filter(|r| !r.is_empty()) else { return Ok(None) };
    let ok = r.split_once('-').is_some_and(
        |(a, b)| matches!((a.trim().parse::<u32>(), b.trim().parse::<u32>()), (Ok(lo), Ok(hi)) if lo <= hi),
    );
    if ok { Ok(Some(r.replace(' ', ""))) } else { Err(Error::bad_request("a rating range looks like 1500-1800")) }
}

/// Checks a Stockfish level: Lichess's AI plays at 1 to 8, and only casual games.
pub fn check_ai(level: u8, rated: bool) -> Result<(), Error> {
    if !(1..=8).contains(&level) {
        return Err(Error::bad_request("the AI's level is 1 to 8"));
    }
    if rated {
        return Err(Error::bad_request("games against the AI are casual"));
    }
    Ok(())
}

/// What you may do in your game on Lichess, by its rules: move on your turn; abort until both sides have moved and
/// resign after; offer a draw or ask for a takeback (either turn, from the second ply, one at a time) and answer the
/// opponent's; claim the game or a draw once the opponent has been gone long enough (`can_claim`).
pub fn actions(you: Side, turn: Side, plies: u32, offers: Offers, can_claim: bool) -> Vec<SeatAction> {
    let mut actions = Vec::new();
    if turn == you {
        actions.push(SeatAction::Move);
    }
    let them = you.other();
    if plies >= 2 {
        match offers.draw {
            Some(s) if s == them => actions.extend([SeatAction::AcceptDraw, SeatAction::DeclineDraw]),
            Some(_) => {}
            None => actions.push(SeatAction::OfferDraw),
        }
        match offers.takeback {
            Some(s) if s == them => actions.extend([SeatAction::AcceptTakeback, SeatAction::DeclineTakeback]),
            Some(_) => {}
            None => actions.push(SeatAction::Takeback),
        }
    }
    actions.push(if plies < 2 { SeatAction::Abort } else { SeatAction::Resign });
    if can_claim {
        actions.extend([SeatAction::ClaimVictory, SeatAction::ClaimDraw]);
    }
    actions
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn challenges_are_blitz_or_slower_with_lichess_clocks() {
        assert!(check_challenge(180, 0, false).is_ok());
        assert!(check_challenge(60, 3, false).is_ok()); // 60 + 120 = 180: blitz
        assert!(check_challenge(120, 1, false).is_err()); // 160: bullet
        assert!(check_challenge(100, 5, false).is_err()); // not a Lichess clock
        assert!(check_challenge(300, 181, false).is_err());
        assert!(check_challenge(300, 61, true).is_err());
        assert!(check_challenge(300, 60, true).is_ok());
        assert!(check_challenge(MAX_INITIAL + 60, 0, false).is_err());
    }

    #[test]
    fn seeks_are_rapid_or_slower() {
        assert!(check_seek(600, 0).is_ok());
        assert!(check_seek(300, 5).is_ok()); // 300 + 200 = 500
        assert!(check_seek(300, 3).is_err()); // 420: blitz
        assert!(check_seek(MAX_INITIAL + 1, 0).is_err());
        assert_eq!(rating_range(Some(" 1500-1800 ")).unwrap().as_deref(), Some("1500-1800"));
        assert_eq!(rating_range(Some("")).unwrap(), None);
        assert!(rating_range(Some("1800-1500")).is_err());
        assert!(rating_range(Some("high")).is_err());
    }

    #[test]
    fn your_actions_follow_lichess() {
        let none = Offers::default();
        assert_eq!(actions(Side::White, Side::White, 0, none, false), [SeatAction::Move, SeatAction::Abort]);
        assert_eq!(actions(Side::Black, Side::White, 1, none, false), [SeatAction::Abort]);
        let a = actions(Side::White, Side::Black, 10, Offers { draw: Some(Side::Black), takeback: None }, true);
        assert_eq!(
            a,
            [
                SeatAction::AcceptDraw,
                SeatAction::DeclineDraw,
                SeatAction::Takeback,
                SeatAction::Resign,
                SeatAction::ClaimVictory,
                SeatAction::ClaimDraw
            ]
        );
        let mine = actions(
            Side::White,
            Side::White,
            10,
            Offers { draw: Some(Side::White), takeback: Some(Side::White) },
            false,
        );
        assert_eq!(mine, [SeatAction::Move, SeatAction::Resign]);
    }

    #[test]
    fn only_some_games_are_playable_here() {
        assert!(!playable(Speed::Bullet, "standard"));
        assert!(playable(Speed::Blitz, "fromPosition"));
        assert!(!playable(Speed::Rapid, "chess960"));
        assert!(playable(Speed::Correspondence, "standard"));
        assert!(check_ai(8, false).is_ok());
        assert!(check_ai(9, false).is_err());
        assert!(check_ai(3, true).is_err());
    }
}
