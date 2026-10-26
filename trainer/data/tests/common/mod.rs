//! Generated games for the tests: random legal play from standard and DFRC starts, with random scores.

#![allow(dead_code)]

use data::{
    game::{Entry, Game, GameResult},
    openings::{Opening, Openings},
    rng::Rng,
};
use shakmaty::Position;

/// `count` games, alternating standard and DFRC starts (`standard_only` for binpack), up to `max_plies` each.
pub fn random_games(count: usize, seed: u64, max_plies: usize, standard_only: bool) -> Vec<Game> {
    let mut rng = Rng::new(seed);
    let standard = Openings::new(&Opening::Startpos).unwrap();
    let dfrc = Openings::new(&Opening::Dfrc).unwrap();
    (0..count)
        .map(|i| {
            let openings = if standard_only || i % 2 == 0 { &standard } else { &dfrc };
            let start = openings.start(&mut rng, (i % 5) as u32).unwrap();
            let result = match rng.below(3) {
                0 => GameResult::BlackWins,
                1 => GameResult::Draw,
                _ => GameResult::WhiteWins,
            };
            let mut game = Game::new(start.clone(), result);
            let mut pos = start;
            let plies = 1 + rng.below(max_plies as u64) as usize;
            for _ in 0..plies {
                let moves = pos.legal_moves();
                if moves.is_empty() {
                    break;
                }
                // Prefer captures and promotions now and then, so they show up.
                let special: Vec<_> =
                    moves.iter().filter(|m| m.is_capture() || m.is_promotion() || m.is_castle()).collect();
                let mv = if !special.is_empty() && rng.below(3) == 0 {
                    *special[rng.below(special.len() as u64) as usize]
                } else {
                    moves[rng.below(moves.len() as u64) as usize]
                };
                let score = (rng.below(4001) as i32 - 2000) as i16;
                game.entries.push(Entry { mv, score });
                pos.play_unchecked(mv);
            }
            game
        })
        .collect()
}

/// The fake engine built with this package.
pub fn fake_engine() -> std::path::PathBuf {
    std::path::PathBuf::from(env!("CARGO_BIN_EXE_fake-uci"))
}
