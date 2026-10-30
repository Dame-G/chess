//! Profiles of players and comparisons: a player's stored games (and their newest reviews) added up by `core`'s
//! [`profile`], and profiles side by side by `core`'s [`compare`]. These only read; the searching was the reviews'.
//!
//! Players are You, engines of the engines list (and engines that played here before, such as Reckless) and people on
//! Lichess. Weak spots (positions from your mistakes, to play out again) are found for You only.

use std::sync::Arc;

use dameless_core::{
    ConnectionKey, Error, PlayerKey,
    lichess::Account,
    profile::{self, Comparison, Player, Profile, ProfileFilter},
};
use dameless_hub::Hub;
use dameless_store::ConnectionField;

/// An engine's name from its id, for one no longer on the list: `reckless` is Reckless.
fn engine_name(id: &str) -> String {
    id.split('-')
        .map(|w| {
            let mut c = w.chars();
            c.next().map(|f| f.to_uppercase().chain(c).collect::<String>()).unwrap_or_default()
        })
        .collect::<Vec<_>>()
        .join(" ")
}

/// Players a comparison takes, at most.
pub const MAX_COMPARED: usize = 8;

/// Profiles and comparisons. Cheap to clone.
#[derive(Clone)]
pub struct Profiles {
    hub: Arc<Hub>,
}

impl Profiles {
    pub fn new(hub: Arc<Hub>) -> Self {
        Self { hub }
    }

    pub(crate) fn hub(&self) -> &Arc<Hub> {
        &self.hub
    }

    /// What is known of a player: the players table first (Lichess people, as imports record them), then your Lichess
    /// account for You, the engines list for an engine, then the name they sat at a board under.
    pub async fn player(&self, key: &PlayerKey) -> Result<Player, Error> {
        let store = &self.hub.store;
        if let Some(p) = store.player(key).await? {
            return Ok(p);
        }
        let mut player =
            Player { key: key.clone(), name: String::new(), title: None, ratings: Default::default(), lichess: None };
        match key {
            PlayerKey::You => {
                player.name = "You".into();
                let account: Option<Account> =
                    store.connection_field(ConnectionKey::You, ConnectionField::Account).await.ok().flatten();
                if let Some(a) = account {
                    player.name = a.username.clone();
                    player.title = a.title;
                    player.ratings = a.ratings;
                    player.lichess = Some(a.username);
                }
            }
            PlayerKey::Engine(id) => {
                player.name = match store.engine(id).await? {
                    Some(e) => e.name,
                    None => match dameless_core::engines::builtin(id) {
                        Some(b) => b.name.to_owned(),
                        None => engine_name(id),
                    },
                };
            }
            PlayerKey::Lichess(id) => {
                let newest = store.profile_games(key.clone(), ProfileFilter { limit: 1, ..Default::default() }, false);
                let seat = newest.await?.into_iter().next().and_then(|g| {
                    let side = g.summary.side_of(key)?;
                    Some(g.summary.seat(side).clone())
                });
                player.name = seat.as_ref().map_or_else(|| id.clone(), |s| s.name.clone());
                player.title = seat.and_then(|s| s.title);
                player.lichess = Some(id.clone());
            }
        }
        Ok(player)
    }

    /// A player's games added up: results over every finished game that passes the filter, engine figures over the
    /// reviewed ones, and weak spots for You.
    pub async fn profile(&self, key: PlayerKey, filter: ProfileFilter) -> Result<Profile, Error> {
        let player = self.player(&key).await?;
        // Moments and weak spots read the reviewed moves, so the plies come along.
        let games = self.hub.store.profile_games(key.clone(), filter.clone(), true).await?;
        Ok(profile::profile(player, filter, &games, key == PlayerKey::You))
    }

    /// Profiles side by side with who leads each row. Two to eight players, each once.
    pub async fn compare(&self, keys: Vec<PlayerKey>, filter: ProfileFilter) -> Result<Comparison, Error> {
        if keys.len() < 2 || keys.len() > MAX_COMPARED {
            return Err(Error::bad_request(format!("compare 2 to {MAX_COMPARED} players")));
        }
        if keys.iter().enumerate().any(|(i, k)| keys[..i].contains(k)) {
            return Err(Error::bad_request("each player once"));
        }
        let mut profiles = Vec::with_capacity(keys.len());
        for key in keys {
            profiles.push(self.profile(key, filter.clone()).await?);
        }
        Ok(profile::compare(profiles))
    }
}
