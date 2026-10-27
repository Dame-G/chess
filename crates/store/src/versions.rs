//! Versions of Dameless and the binaries built from them. A version is named once (`v14`, `deeper-lmr`); builds are a
//! cache keyed by commit and cargo features.

use dameless_core::{
    BuildId, Error, VersionId,
    training::NetId,
    upstream::UpstreamMerge,
    version::{Build, BuildStatus, Estimate, Line, Version},
};
use rusqlite::{Connection, OptionalExtension, Row, params};

use crate::{DbResult, Store, from_json, from_opt_json, from_word, now, opt_json, to_json, word};

/// A version to record. Upserted by name: recording a name again moves it to the new commit and line.
#[derive(Clone, Debug, PartialEq)]
pub struct NewVersion {
    pub name: String,
    pub commit: String,
    pub branch: Option<String>,
    pub line: Line,
    pub parent: Option<VersionId>,
    pub bench: Option<u64>,
    /// The net its builds embed; none for the commit's own.
    pub net: Option<NetId>,
}

impl NewVersion {
    pub fn new(name: impl Into<String>, commit: impl Into<String>, line: Line) -> Self {
        Self { name: name.into(), commit: commit.into(), branch: None, line, parent: None, bench: None, net: None }
    }
}

/// A build to record.
#[derive(Clone, Debug, PartialEq)]
pub struct NewBuild {
    pub version: VersionId,
    pub commit: String,
    pub features: Vec<String>,
    /// The net it embeds, when not the commit's own.
    pub net: Option<NetId>,
    pub path: String,
    pub status: BuildStatus,
    pub log_path: Option<String>,
}

fn line_kind(line: &Line) -> &'static str {
    match line {
        Line::Champion { .. } => "champion",
        Line::Idea { .. } => "idea",
        Line::Upstream => "upstream",
        Line::Other => "other",
    }
}

/// Features as one comparable word: sorted, without repeats, joined by commas (`""` for none).
fn features_key(features: &[String]) -> String {
    let mut f: Vec<&str> = features.iter().map(|s| s.trim()).filter(|s| !s.is_empty()).collect();
    f.sort_unstable();
    f.dedup();
    f.join(",")
}

const VERSION: &str =
    "SELECT id, name, commit_sha, branch, line, parent, bench, elo, created_at, net, upstream FROM versions";

fn version_from_row(row: &Row<'_>) -> DbResult<Version> {
    Ok(Version {
        id: VersionId(row.get(0)?),
        name: row.get(1)?,
        commit: row.get(2)?,
        branch: row.get(3)?,
        line: from_json(&row.get::<_, String>(4)?)?,
        parent: row.get::<_, Option<i64>>(5)?.map(VersionId),
        bench: row.get::<_, Option<i64>>(6)?.map(|b| b as u64),
        elo: from_opt_json(row.get(7)?)?,
        net: row.get::<_, Option<i64>>(9)?.map(NetId),
        upstream: from_opt_json(row.get(10)?)?,
        created_at: dameless_core::Timestamp(row.get(8)?),
    })
}

fn versions_where(conn: &Connection, clause: &str, args: impl rusqlite::Params) -> DbResult<Vec<Version>> {
    let mut stmt = conn.prepare_cached(&format!("{VERSION} {clause}"))?;
    let mut rows = stmt.query(args)?;
    let mut out = Vec::new();
    while let Some(row) = rows.next()? {
        out.push(version_from_row(row)?);
    }
    Ok(out)
}

const BUILD: &str = "SELECT id, version, features, bench, nps, path, status, deleted_at, net FROM builds";

fn build_from_row(row: &Row<'_>) -> DbResult<Build> {
    let features: String = row.get(2)?;
    let deleted: Option<String> = row.get(7)?;
    Ok(Build {
        id: BuildId(row.get(0)?),
        version: VersionId(row.get(1)?),
        features: features.split(',').filter(|f| !f.is_empty()).map(str::to_owned).collect(),
        net: row.get::<_, Option<i64>>(8)?.map(NetId),
        bench: row.get::<_, Option<i64>>(3)?.map(|b| b as u64),
        nps: row.get::<_, Option<i64>>(4)?.map(|b| b as u64),
        path: row.get(5)?,
        status: if deleted.is_some() { BuildStatus::Deleted } else { from_word(&row.get::<_, String>(6)?)? },
    })
}

fn builds_where(conn: &Connection, clause: &str, args: impl rusqlite::Params) -> DbResult<Vec<Build>> {
    let mut stmt = conn.prepare_cached(&format!("{BUILD} {clause}"))?;
    let mut rows = stmt.query(args)?;
    let mut out = Vec::new();
    while let Some(row) = rows.next()? {
        out.push(build_from_row(row)?);
    }
    Ok(out)
}

impl Store {
    /// Records a version by name, or moves an existing name to this commit and line (keeping its id, Elo and when it
    /// was first recorded). Returns it as stored.
    pub async fn upsert_version(&self, v: NewVersion) -> Result<Version, Error> {
        self.write(move |tx| {
            let champion = match v.line {
                Line::Champion { number } => Some(number),
                _ => None,
            };
            tx.execute(
                "INSERT INTO versions (name, commit_sha, branch, line, line_kind, champion, parent, bench, created_at, net) \
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10) \
                 ON CONFLICT (name) DO UPDATE SET commit_sha = excluded.commit_sha, branch = excluded.branch, \
                 line = excluded.line, line_kind = excluded.line_kind, champion = excluded.champion, \
                 parent = excluded.parent, bench = excluded.bench, net = excluded.net",
                params![
                    v.name,
                    v.commit,
                    v.branch,
                    to_json(&v.line)?,
                    line_kind(&v.line),
                    champion,
                    v.parent.map(|p| p.0),
                    v.bench.map(|b| b as i64),
                    now().as_str(),
                    v.net.map(|n| n.0),
                ],
            )?;
            versions_where(tx, "WHERE name = ?1", [&v.name])?
                .pop()
                .ok_or_else(|| Error::internal("a version vanished as it was written").into())
        })
        .await
    }

    /// Records the Elo a version's deciding trial measured against its parent.
    pub async fn set_version_elo(&self, id: VersionId, elo: Option<Estimate>) -> Result<(), Error> {
        self.write(move |tx| {
            let n = tx.execute("UPDATE versions SET elo = ?2 WHERE id = ?1", params![id.0, opt_json(&elo)?])?;
            if n == 0 { Err(Error::not_found(format!("No version {id}")).into()) } else { Ok(()) }
        })
        .await
    }

    /// Records what a champion taken from upstream brought.
    pub async fn set_version_upstream(&self, id: VersionId, merge: Option<UpstreamMerge>) -> Result<(), Error> {
        self.write(move |tx| {
            let n = tx.execute("UPDATE versions SET upstream = ?2 WHERE id = ?1", params![id.0, opt_json(&merge)?])?;
            if n == 0 { Err(Error::not_found(format!("No version {id}")).into()) } else { Ok(()) }
        })
        .await
    }

    pub async fn version(&self, id: VersionId) -> Result<Option<Version>, Error> {
        self.read(move |conn| Ok(versions_where(conn, "WHERE id = ?1", [id.0])?.pop())).await
    }

    pub async fn version_by_name(&self, name: &str) -> Result<Option<Version>, Error> {
        let name = name.to_owned();
        self.read(move |conn| Ok(versions_where(conn, "WHERE name = ?1", [name])?.pop())).await
    }

    /// Versions of a commit (one commit can be on two lines, an idea's tip promoted to champion).
    pub async fn versions_by_commit(&self, commit: &str) -> Result<Vec<Version>, Error> {
        let commit = commit.to_owned();
        self.read(move |conn| versions_where(conn, "WHERE commit_sha = ?1 ORDER BY id", [commit])).await
    }

    /// Every version, or one line's (`champion`, `idea`, `upstream`, `other`), newest first; the champion line in
    /// order of promotion, newest first.
    pub async fn versions(&self, line: Option<&str>) -> Result<Vec<Version>, Error> {
        let line = line.map(str::to_owned);
        self.read(move |conn| {
            versions_where(
                conn,
                "WHERE ?1 IS NULL OR line_kind = ?1 ORDER BY champion IS NULL, champion DESC, created_at DESC, id DESC",
                [line],
            )
        })
        .await
    }

    /// The current champion: the highest number on the champion line.
    pub async fn champion(&self) -> Result<Option<Version>, Error> {
        self.read(move |conn| {
            Ok(versions_where(conn, "WHERE champion IS NOT NULL ORDER BY champion DESC LIMIT 1", [])?.pop())
        })
        .await
    }

    /// Records a build; returns it.
    pub async fn insert_build(&self, b: NewBuild) -> Result<Build, Error> {
        self.write(move |tx| {
            tx.execute(
                "INSERT INTO builds (version, commit_sha, features, path, status, log_path, created_at, net) \
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
                params![
                    b.version.0,
                    b.commit,
                    features_key(&b.features),
                    b.path,
                    word(&b.status),
                    b.log_path,
                    now().as_str(),
                    b.net.map(|n| n.0)
                ],
            )?;
            let id = tx.last_insert_rowid();
            builds_where(tx, "WHERE id = ?1", [id])?.pop().ok_or_else(|| Error::internal("a build vanished").into())
        })
        .await
    }

    /// A build's progress: its status and, once benched, its bench and speed.
    pub async fn update_build(
        &self,
        id: BuildId,
        status: BuildStatus,
        bench: Option<u64>,
        nps: Option<u64>,
    ) -> Result<Build, Error> {
        self.write(move |tx| {
            let n = tx.execute(
                "UPDATE builds SET status = ?2, bench = coalesce(?3, bench), nps = coalesce(?4, nps) WHERE id = ?1",
                params![id.0, word(&status), bench.map(|b| b as i64), nps.map(|n| n as i64)],
            )?;
            if n == 0 {
                return Err(Error::not_found(format!("No build {id}")).into());
            }
            builds_where(tx, "WHERE id = ?1", [id.0])?.pop().ok_or_else(|| Error::internal("a build vanished").into())
        })
        .await
    }

    /// Marks a build's binary deleted; the record stays for what used it.
    pub async fn mark_build_deleted(&self, id: BuildId) -> Result<(), Error> {
        self.write(move |tx| {
            let n = tx.execute(
                "UPDATE builds SET status = 'deleted', deleted_at = coalesce(deleted_at, ?2) WHERE id = ?1",
                params![id.0, now().as_str()],
            )?;
            if n == 0 { Err(Error::not_found(format!("No build {id}")).into()) } else { Ok(()) }
        })
        .await
    }

    pub async fn build(&self, id: BuildId) -> Result<Option<Build>, Error> {
        self.read(move |conn| Ok(builds_where(conn, "WHERE id = ?1", [id.0])?.pop())).await
    }

    /// A version's builds, newest first.
    pub async fn builds(&self, version: VersionId) -> Result<Vec<Build>, Error> {
        self.read(move |conn| builds_where(conn, "WHERE version = ?1 ORDER BY id DESC", [version.0])).await
    }

    /// The cache: the newest ready, not deleted build of a commit with exactly these features and the commit's own
    /// net.
    pub async fn cached_build(&self, commit: &str, features: &[String]) -> Result<Option<Build>, Error> {
        self.cached_build_with_net(commit, features, None).await
    }

    /// The cache, keyed by (commit, features, net): `None` is the commit's own net.
    pub async fn cached_build_with_net(
        &self,
        commit: &str,
        features: &[String],
        net: Option<NetId>,
    ) -> Result<Option<Build>, Error> {
        let (commit, features) = (commit.to_owned(), features_key(features));
        self.read(move |conn| {
            Ok(builds_where(
                conn,
                "WHERE commit_sha = ?1 AND features = ?2 AND net IS ?3 AND status = 'ready' AND deleted_at IS NULL \
                 ORDER BY id DESC LIMIT 1",
                params![commit, features, net.map(|n| n.0)],
            )?
            .pop())
        })
        .await
    }

    /// Where a build's log is.
    pub async fn build_log_path(&self, id: BuildId) -> Result<Option<String>, Error> {
        self.read(move |conn| {
            Ok(conn
                .query_row("SELECT log_path FROM builds WHERE id = ?1", [id.0], |r| r.get::<_, Option<String>>(0))
                .optional()?
                .flatten())
        })
        .await
    }
}

#[cfg(test)]
mod tests {
    use dameless_core::IdeaId;

    use super::*;
    use crate::testing;

    #[tokio::test]
    async fn versions_upsert_by_name_and_the_champion_is_the_highest() {
        let (store, _dir) = testing::store();
        assert!(store.champion().await.unwrap().is_none());
        let v1 = store.upsert_version(NewVersion::new("v1", "aaa", Line::Champion { number: 1 })).await.unwrap();
        let mut v2 = NewVersion::new("v2", "bbb", Line::Champion { number: 2 });
        v2.parent = Some(v1.id);
        v2.bench = Some(1_234_567);
        let v2 = store.upsert_version(v2).await.unwrap();
        let idea =
            store.upsert_version(NewVersion::new("deeper-lmr", "ccc", Line::Idea { idea: IdeaId(4) })).await.unwrap();
        assert_eq!(store.champion().await.unwrap().map(|v| v.id), Some(v2.id));
        assert_eq!(v2.bench, Some(1_234_567));

        let moved =
            store.upsert_version(NewVersion::new("deeper-lmr", "ddd", Line::Idea { idea: IdeaId(4) })).await.unwrap();
        assert_eq!((moved.id, moved.commit.as_str()), (idea.id, "ddd"));
        store.set_version_elo(v2.id, Some(Estimate::plus_minus(3.0, 2.0))).await.unwrap();
        assert_eq!(store.version(v2.id).await.unwrap().unwrap().elo.map(|e| e.elo), Some(3.0));
        assert_eq!(v2.upstream, None);
        let merge = dameless_core::upstream::UpstreamMerge {
            base: "aaa".into(),
            upstream: "bbb".into(),
            commits: vec![dameless_core::upstream::UpstreamCommit { sha: "bbb".into(), subject: "Gain".into() }],
            fast_forward: true,
        };
        store.set_version_upstream(v2.id, Some(merge.clone())).await.unwrap();
        assert_eq!(store.version(v2.id).await.unwrap().unwrap().upstream, Some(merge));

        let champions: Vec<_> = store.versions(Some("champion")).await.unwrap().into_iter().map(|v| v.name).collect();
        assert_eq!(champions, ["v2", "v1"]);
        assert_eq!(store.versions(None).await.unwrap().len(), 3);
        assert_eq!(store.version_by_name("v1").await.unwrap().map(|v| v.id), Some(v1.id));
        assert_eq!(store.versions_by_commit("aaa").await.unwrap().len(), 1);
        // Two versions cannot share a champion number.
        assert!(store.upsert_version(NewVersion::new("v2b", "eee", Line::Champion { number: 2 })).await.is_err());
    }

    #[tokio::test]
    async fn builds_are_a_cache_by_commit_and_features() {
        let (store, _dir) = testing::store();
        let v = store.upsert_version(NewVersion::new("v1", "aaa", Line::Champion { number: 1 })).await.unwrap();
        let new = |features: &[&str]| NewBuild {
            version: v.id,
            commit: "aaa".into(),
            features: features.iter().map(|s| s.to_string()).collect(),
            net: None,
            path: "builds/aaa".into(),
            status: BuildStatus::Building,
            log_path: Some("logs/build.log".into()),
        };
        let plain = store.insert_build(new(&[])).await.unwrap();
        let spsa = store.insert_build(new(&["spsa"])).await.unwrap();
        assert!(store.cached_build("aaa", &[]).await.unwrap().is_none(), "not ready yet");
        let ready = store.update_build(plain.id, BuildStatus::Ready, Some(1_234_567), Some(1_000_000)).await.unwrap();
        assert_eq!((ready.bench, ready.nps), (Some(1_234_567), Some(1_000_000)));
        store.update_build(spsa.id, BuildStatus::Ready, None, None).await.unwrap();
        assert_eq!(store.cached_build("aaa", &[]).await.unwrap().map(|b| b.id), Some(plain.id));
        let found = store.cached_build("aaa", &["spsa".into()]).await.unwrap().unwrap();
        assert_eq!((found.id, found.features.clone()), (spsa.id, vec!["spsa".to_owned()]));
        assert_eq!(store.builds(v.id).await.unwrap().len(), 2);
        assert_eq!(store.build_log_path(plain.id).await.unwrap().as_deref(), Some("logs/build.log"));

        store.mark_build_deleted(plain.id).await.unwrap();
        assert!(store.cached_build("aaa", &[]).await.unwrap().is_none());
        assert_eq!(store.build(plain.id).await.unwrap().unwrap().status, BuildStatus::Deleted);
        assert_eq!(store.mark_build_deleted(BuildId(99)).await.unwrap_err().status(), 404);
    }
}
