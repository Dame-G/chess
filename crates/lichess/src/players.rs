//! Other players: a public profile (`GET /api/user/{name}`) and the leaderboards (`GET /api/player/top/{nb}/{perf}`),
//! kept ten minutes. Neither needs a token.

use std::time::Duration;

use serde::Deserialize;
use serde_json::Value;

use crate::{Error, Lichess, User, client::seg};

/// How long a profile or a leaderboard is kept.
const KEEP_FOR: Duration = Duration::from_secs(600);

/// The perfs Lichess keeps a leaderboard of.
pub const LEADERBOARD_PERFS: [&str; 5] = ["ultraBullet", "bullet", "blitz", "rapid", "classical"];

impl Lichess {
    /// A player's public profile. `Error::Refused` with 404 when Lichess has no such player.
    pub async fn user(&self, username: &str) -> Result<User, Error> {
        let v = self.cached(&format!("/api/user/{}", seg(&username.to_lowercase()))).await?;
        serde_json::from_value(v).map_err(|e| Error::Unexpected(format!("a profile: {e}")))
    }

    /// The best `nb` players (1 to 200) at a perf: each with that perf's rating and progress.
    pub async fn leaderboard(&self, nb: u32, perf: &str) -> Result<Vec<User>, Error> {
        #[derive(Deserialize)]
        struct Top {
            #[serde(default)]
            users: Vec<User>,
        }
        let v = self.cached(&format!("/api/player/top/{}/{}", nb.clamp(1, 200), seg(perf))).await?;
        let top: Top = serde_json::from_value(v).map_err(|e| Error::Unexpected(format!("a leaderboard: {e}")))?;
        Ok(top.users)
    }

    async fn cached(&self, path: &str) -> Result<Value, Error> {
        if let Some(v) = self.cache().get(path) {
            return Ok(v);
        }
        let v: Value = self.get(path).await?;
        self.cache().put(path.to_owned(), KEEP_FOR, v.clone());
        Ok(v)
    }
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use crate::testing::MockLichess;

    #[tokio::test]
    async fn reads_a_profile_once_in_ten_minutes() {
        let mock = MockLichess::start().await;
        mock.set_user(json!({
            "id": "cutemouse83", "username": "CuteMouse83", "title": "FM",
            "perfs": { "blitz": { "rating": 2400, "games": 3000, "rd": 45, "prog": 12 }, "rapid": { "rating": 2300, "games": 10, "prov": true } },
            "count": { "all": 3100, "rated": 3000, "win": 1600, "draw": 200, "loss": 1300 },
            "profile": { "flag": "FR" }, "createdAt": 1_500_000_000_000i64, "seenAt": 1_700_000_000_000i64
        }));
        let client = mock.client(None);
        let p = client.user("CuteMouse83").await.unwrap();
        assert_eq!((p.title.as_deref(), p.rating("blitz")), (Some("FM"), Some(2400)));
        assert!(p.perfs["rapid"].prov);
        assert_eq!(p.count.as_ref().unwrap().win, 1600);
        assert_eq!(p.flag(), Some("FR"));
        assert_eq!(p.created_at, Some(1_500_000_000_000));
        client.user("cutemouse83").await.unwrap();
        assert_eq!(mock.requests_to("/api/user/").len(), 1);
        assert_eq!(client.user("nobody").await.unwrap_err().status(), Some(404));
    }

    #[tokio::test]
    async fn reads_a_leaderboard() {
        let mock = MockLichess::start().await;
        mock.set_leaderboard(
            "blitz",
            vec![
                json!({ "id": "a", "username": "A", "title": "GM", "perfs": { "blitz": { "rating": 3100, "progress": 20 } } }),
                json!({ "id": "b", "username": "B", "perfs": { "blitz": { "rating": 3050, "progress": -4 } } }),
            ],
        );
        let top = mock.client(None).leaderboard(1, "blitz").await.unwrap();
        assert_eq!(top.len(), 1);
        assert_eq!((top[0].rating("blitz"), top[0].perfs["blitz"].progress), (Some(3100), Some(20)));
        let top = mock.client(None).leaderboard(10, "blitz").await.unwrap();
        assert_eq!(top[1].perfs["blitz"].progress, Some(-4));
    }
}
