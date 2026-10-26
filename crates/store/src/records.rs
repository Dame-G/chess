//! Small keyed records: the Lichess connections' account, settings and state (never their tokens, which are files),
//! players, and `meta`, the app's own settings and counters.

use dameless_core::{ConnectionKey, Error, PlayerKey, Timestamp, profile::Player};
use rusqlite::{OptionalExtension, params};
use serde::{Serialize, de::DeserializeOwned};

use crate::{Store, from_json, from_opt_json, now, to_json};

/// One of a connection's JSON columns.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum ConnectionField {
    /// The Lichess account it is linked to.
    Account,
    /// What it is set to do: the bot's rules, version and seek settings.
    Settings,
    /// What it was doing, to come back to after a restart: online, seeking.
    State,
}

impl ConnectionField {
    fn column(self) -> &'static str {
        match self {
            Self::Account => "account",
            Self::Settings => "settings",
            Self::State => "state",
        }
    }
}

/// A connection's row, its columns as JSON.
#[derive(Clone, Debug, PartialEq)]
pub struct ConnectionRecord {
    pub account: Option<serde_json::Value>,
    pub settings: Option<serde_json::Value>,
    pub state: Option<serde_json::Value>,
    pub updated_at: Timestamp,
}

impl Store {
    /// One field of a connection, read as `T`.
    pub async fn connection_field<T: DeserializeOwned + Send + 'static>(
        &self,
        key: ConnectionKey,
        field: ConnectionField,
    ) -> Result<Option<T>, Error> {
        self.read(move |conn| {
            let text: Option<Option<String>> = conn
                .query_row(&format!("SELECT {} FROM connections WHERE key = ?1", field.column()), [key.as_str()], |r| {
                    r.get(0)
                })
                .optional()?;
            from_opt_json(text.flatten())
        })
        .await
    }

    /// Sets one field of a connection (`None` clears it), creating its row when needed.
    pub async fn put_connection_field<T: Serialize>(
        &self,
        key: ConnectionKey,
        field: ConnectionField,
        value: Option<&T>,
    ) -> Result<(), Error> {
        let text = value.map(to_json).transpose().map_err(Error::from)?;
        self.write(move |tx| {
            let column = field.column();
            tx.execute(
                &format!(
                    "INSERT INTO connections (key, {column}, updated_at) VALUES (?1, ?2, ?3) \
                     ON CONFLICT (key) DO UPDATE SET {column} = excluded.{column}, updated_at = excluded.updated_at"
                ),
                params![key.as_str(), text, now().as_str()],
            )?;
            Ok(())
        })
        .await
    }

    /// A connection's whole row.
    pub async fn connection(&self, key: ConnectionKey) -> Result<Option<ConnectionRecord>, Error> {
        self.read(move |conn| {
            let mut stmt =
                conn.prepare_cached("SELECT account, settings, state, updated_at FROM connections WHERE key = ?1")?;
            let mut rows = stmt.query([key.as_str()])?;
            let Some(row) = rows.next()? else { return Ok(None) };
            Ok(Some(ConnectionRecord {
                account: from_opt_json(row.get(0)?)?,
                settings: from_opt_json(row.get(1)?)?,
                state: from_opt_json(row.get(2)?)?,
                updated_at: Timestamp(row.get(3)?),
            }))
        })
        .await
    }

    /// Forgets a connection's row (logging out).
    pub async fn delete_connection(&self, key: ConnectionKey) -> Result<(), Error> {
        self.write(move |tx| {
            tx.execute("DELETE FROM connections WHERE key = ?1", [key.as_str()])?;
            Ok(())
        })
        .await
    }

    /// Records what is known of a player, replacing what was.
    pub async fn upsert_player(&self, player: Player) -> Result<(), Error> {
        self.write(move |tx| {
            tx.execute(
                "INSERT INTO players (key, name, title, lichess, ratings, updated_at) VALUES (?1, ?2, ?3, ?4, ?5, ?6) \
                 ON CONFLICT (key) DO UPDATE SET name = excluded.name, title = excluded.title, \
                 lichess = excluded.lichess, ratings = excluded.ratings, updated_at = excluded.updated_at",
                params![
                    player.key.to_string(),
                    player.name,
                    player.title,
                    player.lichess,
                    to_json(&player.ratings)?,
                    now().as_str()
                ],
            )?;
            Ok(())
        })
        .await
    }

    pub async fn player(&self, key: &PlayerKey) -> Result<Option<Player>, Error> {
        let key = key.clone();
        self.read(move |conn| {
            let row: Option<(String, Option<String>, Option<String>, String)> = conn
                .query_row("SELECT name, title, lichess, ratings FROM players WHERE key = ?1", [key.to_string()], |r| {
                    Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?))
                })
                .optional()?;
            row.map(|(name, title, lichess, ratings)| {
                Ok(Player { key: key.clone(), name, title, lichess, ratings: from_json(&ratings)? })
            })
            .transpose()
        })
        .await
    }

    /// A value of the app's own, read as `T`.
    pub async fn meta<T: DeserializeOwned + Send + 'static>(&self, key: &str) -> Result<Option<T>, Error> {
        let key = key.to_owned();
        self.read(move |conn| {
            let text: Option<String> =
                conn.query_row("SELECT value FROM meta WHERE key = ?1", [key], |r| r.get(0)).optional()?;
            text.as_deref().map(from_json).transpose()
        })
        .await
    }

    pub async fn put_meta<T: Serialize>(&self, key: &str, value: &T) -> Result<(), Error> {
        let (key, text) = (key.to_owned(), to_json(value).map_err(Error::from)?);
        self.write(move |tx| {
            tx.execute("INSERT OR REPLACE INTO meta (key, value) VALUES (?1, ?2)", params![key, text])?;
            Ok(())
        })
        .await
    }

    pub async fn delete_meta(&self, key: &str) -> Result<(), Error> {
        let key = key.to_owned();
        self.write(move |tx| {
            tx.execute("DELETE FROM meta WHERE key = ?1", [key])?;
            Ok(())
        })
        .await
    }
}

#[cfg(test)]
mod tests {
    use dameless_core::lichess::{Rating, Ratings};

    use super::*;
    use crate::testing;

    #[tokio::test]
    async fn connections_keep_fields_apart() {
        let (store, _dir) = testing::store();
        assert!(store.connection(ConnectionKey::Bot).await.unwrap().is_none());
        let settings = serde_json::json!({ "hash": 64 });
        store.put_connection_field(ConnectionKey::Bot, ConnectionField::Settings, Some(&settings)).await.unwrap();
        store
            .put_connection_field(
                ConnectionKey::Bot,
                ConnectionField::State,
                Some(&serde_json::json!({ "online": true })),
            )
            .await
            .unwrap();
        let got: Option<serde_json::Value> =
            store.connection_field(ConnectionKey::Bot, ConnectionField::Settings).await.unwrap();
        assert_eq!(got, Some(settings.clone()));
        let row = store.connection(ConnectionKey::Bot).await.unwrap().unwrap();
        assert_eq!((row.account, row.settings), (None, Some(settings)));
        store
            .put_connection_field::<serde_json::Value>(ConnectionKey::Bot, ConnectionField::State, None)
            .await
            .unwrap();
        assert_eq!(store.connection(ConnectionKey::Bot).await.unwrap().unwrap().state, None);
        assert!(store.connection(ConnectionKey::You).await.unwrap().is_none());
        store.delete_connection(ConnectionKey::Bot).await.unwrap();
        assert!(store.connection(ConnectionKey::Bot).await.unwrap().is_none());
    }

    #[tokio::test]
    async fn players_and_meta() {
        let (store, _dir) = testing::store();
        let key = PlayerKey::Lichess("bob".into());
        assert!(store.player(&key).await.unwrap().is_none());
        let mut bob = Player {
            key: key.clone(),
            name: "Bob".into(),
            title: Some("FM".into()),
            ratings: Ratings::default(),
            lichess: Some("Bob".into()),
        };
        store.upsert_player(bob.clone()).await.unwrap();
        bob.ratings.blitz = Some(Rating { rating: 2100, games: Some(40), provisional: false });
        store.upsert_player(bob.clone()).await.unwrap();
        assert_eq!(store.player(&key).await.unwrap(), Some(bob));

        assert_eq!(store.meta::<u32>("bot_games_today").await.unwrap(), None);
        store.put_meta("bot_games_today", &12u32).await.unwrap();
        store.put_meta("bot_games_today", &13u32).await.unwrap();
        assert_eq!(store.meta::<u32>("bot_games_today").await.unwrap(), Some(13));
        store.delete_meta("bot_games_today").await.unwrap();
        assert_eq!(store.meta::<u32>("bot_games_today").await.unwrap(), None);
    }
}
