//! Your openings: the lines you play as White and as Black, where you leave theory, and how you score, from your
//! finished games (`core::repertoire::build`). Theory is Lichess's own book, read from your games: a position reached
//! within the named opening's plies (`opening.ply`) of any of your Lichess games.
//!
//! It is assistance, like the explorer: refused (409, fair play) while the policy forbids the explorer.

use std::collections::HashSet;

use dameless_core::{
    Error, PlayerKey, Side,
    chess::Position,
    events::AssistKind,
    game::{Game, GameStatus},
    prep::{self, Prep, PrepRequest},
    profile::ProfileFilter,
    repertoire::{self, Repertoire, RepertoireGame, RepertoireOptions},
};
use dameless_store::GameFilter;
use serde::{Deserialize, Serialize};
use utoipa::{IntoParams, ToSchema};

use crate::Profiles;

/// Your games a repertoire reads when the request gives no `limit`, newest first.
pub const DEFAULT_GAMES: u32 = 500;

/// `GET /api/players/you/repertoire?side=&max_plies=&min_games=&limit=`.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, ToSchema, IntoParams)]
pub struct RepertoireRequest {
    pub side: Side,
    /// How deep the tree goes, in plies (20).
    #[serde(default)]
    pub max_plies: Option<u32>,
    /// A move shows once played in this many games (2).
    #[serde(default)]
    pub min_games: Option<u32>,
    /// Your newest finished games read, at most (500).
    #[serde(default)]
    pub limit: Option<u32>,
}

/// The positions (by key) within the named opening of a game: theory, by Lichess's book.
fn theory_of(game: &Game, into: &mut HashSet<String>) {
    let Some(ply) = game.summary.opening.as_ref().and_then(|o| o.ply) else { return };
    let Ok(mut pos) = Position::from_start(game.start_fen.as_deref()) else { return };
    into.insert(pos.key());
    for m in game.moves.iter().take(ply as usize) {
        if pos.play(&m.uci).is_err() {
            break;
        }
        into.insert(pos.key());
    }
}

impl Profiles {
    /// Your opening lines with one colour, each with how you score and how often you go wrong early (`core::prep`).
    /// Assistance like the repertoire: 409 while you play on Lichess (unless every live game is correspondence).
    pub async fn prep(&self, req: PrepRequest) -> Result<Prep, Error> {
        let hub = self.hub();
        hub.policy.check(AssistKind::Explorer)?;
        let limit = req.limit.unwrap_or(DEFAULT_GAMES).clamp(1, dameless_store::games::MAX_LIMIT);
        let games =
            hub.store.profile_games(PlayerKey::You, ProfileFilter { limit, ..ProfileFilter::default() }, true).await?;
        let (mut prep, plies) = prep::build(req.side, &games);
        for (line, plies) in prep.lines.iter_mut().zip(plies) {
            if let Some(game) = hub.store.game(line.example).await? {
                let moves: Vec<String> = game.moves.iter().map(|m| m.uci.clone()).collect();
                line.set_moves(&moves, plies);
            }
        }
        Ok(prep)
    }

    /// Your repertoire with one colour. 409 while you play on Lichess (unless every live game is correspondence).
    pub async fn repertoire(&self, req: RepertoireRequest) -> Result<Repertoire, Error> {
        let hub = self.hub();
        hub.policy.check(AssistKind::Explorer)?;
        let defaults = RepertoireOptions::default();
        let options = RepertoireOptions {
            max_plies: req.max_plies.unwrap_or(defaults.max_plies).clamp(1, 80),
            min_games: req.min_games.unwrap_or(defaults.min_games).max(1),
        };
        let summaries = hub
            .store
            .games(GameFilter {
                player: Some(PlayerKey::You),
                statuses: vec![GameStatus::Finished],
                limit: req.limit.unwrap_or(DEFAULT_GAMES).clamp(1, dameless_store::games::MAX_LIMIT),
                ..GameFilter::default()
            })
            .await?;
        let mut games = Vec::with_capacity(summaries.len());
        let mut theory = HashSet::new();
        for s in summaries {
            let Some(game) = hub.store.game(s.id).await? else { continue };
            theory_of(&game, &mut theory);
            if let Some(g) = RepertoireGame::from_game(&game, &PlayerKey::You) {
                games.push(g);
            }
        }
        let is_theory = |key: &str| theory.contains(key);
        let theory: Option<&dyn Fn(&str) -> bool> = if theory.is_empty() { None } else { Some(&is_theory) };
        Ok(repertoire::build(req.side, &games, options, theory))
    }
}
