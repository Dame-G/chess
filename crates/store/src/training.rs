//! Training (R4): datasets, nets and the readings of training runs.

use dameless_core::{
    Error, IdeaId, Timestamp, WorkId,
    training::{Dataset, DatasetId, DatasetSource, Net, NetId, NetRun, TrainStep},
};
use rusqlite::{Connection, Row, params};

use crate::{DbResult, Store, from_json, from_opt_json, now, opt_json, to_json};

/// A dataset to record, upserted by name (a datagen run updates its dataset as it grows).
#[derive(Clone, Debug, PartialEq)]
pub struct NewDataset {
    pub name: String,
    pub format: String,
    pub positions: u64,
    pub games: u64,
    pub size_bytes: u64,
    pub source: DatasetSource,
    pub score_units: Option<String>,
    pub path: String,
}

/// A net to record. A file already known by its SHA-256 is that net.
#[derive(Clone, Debug, PartialEq)]
pub struct NewNet {
    pub name: String,
    pub sha256: String,
    pub path: String,
    pub arch: String,
    pub size_bytes: u64,
    pub parent: Option<NetId>,
    pub dataset: Option<DatasetId>,
    pub run: Option<NetRun>,
}

fn source_kind(source: &DatasetSource) -> &'static str {
    match source {
        DatasetSource::Datagen { .. } => "datagen",
        DatasetSource::Import { .. } => "import",
        DatasetSource::Trials { .. } => "trials",
        DatasetSource::Other { .. } => "other",
    }
}

const DATASET: &str = "SELECT id, name, format, positions, games, size_bytes, source, score_units, path, created_at, \
                       updated_at FROM datasets";

fn dataset_from_row(row: &Row<'_>) -> DbResult<Dataset> {
    Ok(Dataset {
        id: DatasetId(row.get(0)?),
        name: row.get(1)?,
        format: row.get(2)?,
        positions: row.get::<_, i64>(3)? as u64,
        games: row.get::<_, i64>(4)? as u64,
        size_bytes: row.get::<_, i64>(5)? as u64,
        source: from_json(&row.get::<_, String>(6)?)?,
        score_units: row.get(7)?,
        path: row.get(8)?,
        created_at: Timestamp(row.get(9)?),
        updated_at: Timestamp(row.get(10)?),
    })
}

fn datasets_where(conn: &Connection, clause: &str, args: impl rusqlite::Params) -> DbResult<Vec<Dataset>> {
    let mut stmt = conn.prepare_cached(&format!("{DATASET} {clause}"))?;
    let mut rows = stmt.query(args)?;
    let mut out = Vec::new();
    while let Some(row) = rows.next()? {
        out.push(dataset_from_row(row)?);
    }
    Ok(out)
}

const NET: &str =
    "SELECT id, name, sha256, path, arch, size_bytes, parent, dataset, run, metrics, created_at FROM nets";

fn net_from_row(row: &Row<'_>) -> DbResult<Net> {
    Ok(Net {
        id: NetId(row.get(0)?),
        name: row.get(1)?,
        sha256: row.get(2)?,
        path: row.get(3)?,
        arch: row.get(4)?,
        size_bytes: row.get::<_, i64>(5)? as u64,
        parent: row.get::<_, Option<i64>>(6)?.map(NetId),
        dataset: row.get::<_, Option<i64>>(7)?.map(DatasetId),
        run: from_opt_json(row.get(8)?)?,
        metrics: from_opt_json(row.get(9)?)?,
        created_at: Timestamp(row.get(10)?),
    })
}

fn nets_where(conn: &Connection, clause: &str, args: impl rusqlite::Params) -> DbResult<Vec<Net>> {
    let mut stmt = conn.prepare_cached(&format!("{NET} {clause}"))?;
    let mut rows = stmt.query(args)?;
    let mut out = Vec::new();
    while let Some(row) = rows.next()? {
        out.push(net_from_row(row)?);
    }
    Ok(out)
}

impl Store {
    /// Records a dataset by name, or updates the one of that name (keeping its id and creation time).
    pub async fn upsert_dataset(&self, d: NewDataset) -> Result<Dataset, Error> {
        self.write(move |tx| {
            let at = now();
            tx.execute(
                "INSERT INTO datasets (name, format, positions, games, size_bytes, source, source_kind, score_units, \
                 path, created_at, updated_at) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?10) \
                 ON CONFLICT (name) DO UPDATE SET format = excluded.format, positions = excluded.positions, \
                 games = excluded.games, size_bytes = excluded.size_bytes, source = excluded.source, \
                 source_kind = excluded.source_kind, score_units = excluded.score_units, path = excluded.path, \
                 updated_at = excluded.updated_at",
                params![
                    d.name,
                    d.format,
                    d.positions as i64,
                    d.games as i64,
                    d.size_bytes as i64,
                    to_json(&d.source)?,
                    source_kind(&d.source),
                    d.score_units,
                    d.path,
                    at.as_str(),
                ],
            )?;
            datasets_where(tx, "WHERE name = ?1", [&d.name])?
                .pop()
                .ok_or_else(|| Error::internal("a dataset vanished as it was written").into())
        })
        .await
    }

    pub async fn dataset(&self, id: DatasetId) -> Result<Option<Dataset>, Error> {
        self.read(move |conn| Ok(datasets_where(conn, "WHERE id = ?1", [id.0])?.pop())).await
    }

    pub async fn dataset_by_name(&self, name: &str) -> Result<Option<Dataset>, Error> {
        let name = name.to_owned();
        self.read(move |conn| Ok(datasets_where(conn, "WHERE name = ?1", [name])?.pop())).await
    }

    /// Every dataset, newest first.
    pub async fn datasets(&self) -> Result<Vec<Dataset>, Error> {
        self.read(|conn| datasets_where(conn, "ORDER BY created_at DESC, id DESC", [])).await
    }

    /// Forgets a dataset (its files are the caller's); nets trained on it keep their record without the link.
    pub async fn delete_dataset(&self, id: DatasetId) -> Result<(), Error> {
        self.write(move |tx| {
            let n = tx.execute("DELETE FROM datasets WHERE id = ?1", [id.0])?;
            if n == 0 { Err(Error::not_found(format!("No dataset {id}")).into()) } else { Ok(()) }
        })
        .await
    }

    /// Records a net, or returns the one already recorded with this SHA-256.
    pub async fn insert_net(&self, n: NewNet) -> Result<Net, Error> {
        let n = NewNet { sha256: n.sha256.to_ascii_lowercase(), ..n };
        self.write(move |tx| {
            if let Some(known) = nets_where(tx, "WHERE sha256 = ?1", [&n.sha256])?.pop() {
                return Ok(known);
            }
            if !nets_where(tx, "WHERE name = ?1", [&n.name])?.is_empty() {
                return Err(Error::conflict(format!("A net named {} already exists", n.name)).into());
            }
            tx.execute(
                "INSERT INTO nets (name, sha256, path, arch, size_bytes, parent, dataset, run, created_at) \
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)",
                params![
                    n.name,
                    n.sha256,
                    n.path,
                    n.arch,
                    n.size_bytes as i64,
                    n.parent.map(|p| p.0),
                    n.dataset.map(|d| d.0),
                    opt_json(&n.run)?,
                    now().as_str(),
                ],
            )?;
            let id = tx.last_insert_rowid();
            nets_where(tx, "WHERE id = ?1", [id])?.pop().ok_or_else(|| Error::internal("a net vanished").into())
        })
        .await
    }

    pub async fn net(&self, id: NetId) -> Result<Option<Net>, Error> {
        self.read(move |conn| Ok(nets_where(conn, "WHERE id = ?1", [id.0])?.pop())).await
    }

    pub async fn net_by_sha(&self, sha256: &str) -> Result<Option<Net>, Error> {
        let sha = sha256.to_ascii_lowercase();
        self.read(move |conn| Ok(nets_where(conn, "WHERE sha256 = ?1", [sha])?.pop())).await
    }

    pub async fn net_by_name(&self, name: &str) -> Result<Option<Net>, Error> {
        let name = name.to_owned();
        self.read(move |conn| Ok(nets_where(conn, "WHERE name = ?1", [name])?.pop())).await
    }

    /// Every net, newest first.
    pub async fn nets(&self) -> Result<Vec<Net>, Error> {
        self.read(|conn| nets_where(conn, "ORDER BY created_at DESC, id DESC", [])).await
    }

    /// Appends readings of a training run; `seq` continues after the last stored for the work item.
    pub async fn add_train_steps(
        &self,
        work: WorkId,
        idea: Option<IdeaId>,
        steps: Vec<TrainStep>,
    ) -> Result<(), Error> {
        self.write(move |tx| {
            let last: i64 =
                tx.query_row("SELECT coalesce(max(seq), 0) FROM train_steps WHERE work = ?1", [work.0], |r| r.get(0))?;
            let mut stmt = tx.prepare_cached(
                "INSERT INTO train_steps (work, seq, idea, superbatch, batch, loss, lr, grad_norm, positions_per_sec, \
                 elapsed_seconds) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10)",
            )?;
            for (i, s) in steps.iter().enumerate() {
                stmt.execute(params![
                    work.0,
                    last + 1 + i as i64,
                    idea.map(|i| i.0),
                    s.superbatch,
                    s.batch,
                    s.loss,
                    s.lr,
                    s.grad_norm,
                    s.positions_per_sec,
                    s.elapsed_seconds,
                ])?;
            }
            Ok(())
        })
        .await
    }

    /// A work item's readings, in order.
    pub async fn train_steps(&self, work: WorkId) -> Result<Vec<TrainStep>, Error> {
        self.read(move |conn| steps_where(conn, "WHERE work = ?1 ORDER BY seq", [work.0])).await
    }

    /// An idea's readings over all its runs (a resumed run carries on in a new work item), numbered again from 1.
    pub async fn idea_train_steps(&self, idea: IdeaId) -> Result<Vec<TrainStep>, Error> {
        let mut steps =
            self.read(move |conn| steps_where(conn, "WHERE idea = ?1 ORDER BY work, seq", [idea.0])).await?;
        for (i, s) in steps.iter_mut().enumerate() {
            s.seq = i as u32 + 1;
        }
        Ok(steps)
    }
}

fn steps_where(conn: &Connection, clause: &str, args: impl rusqlite::Params) -> DbResult<Vec<TrainStep>> {
    let mut stmt = conn.prepare_cached(&format!(
        "SELECT seq, superbatch, batch, loss, lr, grad_norm, positions_per_sec, elapsed_seconds FROM train_steps \
         {clause}"
    ))?;
    let mut rows = stmt.query(args)?;
    let mut out = Vec::new();
    while let Some(r) = rows.next()? {
        out.push(TrainStep {
            seq: r.get(0)?,
            superbatch: r.get(1)?,
            batch: r.get(2)?,
            loss: r.get(3)?,
            lr: r.get(4)?,
            grad_norm: r.get(5)?,
            positions_per_sec: r.get(6)?,
            elapsed_seconds: r.get(7)?,
        });
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use dameless_core::{
        training::RECKLESS_ARCH,
        version::{BuildStatus, Line},
    };

    use super::*;
    use crate::{NewBuild, NewVersion, testing};

    fn dataset(name: &str, positions: u64) -> NewDataset {
        NewDataset {
            name: name.into(),
            format: "viriformat".into(),
            positions,
            games: positions / 100,
            size_bytes: positions * 4,
            source: DatasetSource::Datagen { settings: serde_json::json!({ "soft_nodes": 5000 }) },
            score_units: Some("reckless_internal".into()),
            path: format!("/data/{name}"),
        }
    }

    fn net(name: &str, sha: &str) -> NewNet {
        NewNet {
            name: name.into(),
            sha256: sha.into(),
            path: format!("/nets/{sha}.nnue"),
            arch: RECKLESS_ARCH.into(),
            size_bytes: 63_266_880,
            parent: None,
            dataset: None,
            run: None,
        }
    }

    #[tokio::test]
    async fn datasets_upsert_by_name() {
        let (store, _dir) = testing::store();
        let a = store.upsert_dataset(dataset("dfrc-1", 1000)).await.unwrap();
        let grown = store.upsert_dataset(dataset("dfrc-1", 5000)).await.unwrap();
        assert_eq!((grown.id, grown.positions, grown.created_at.clone()), (a.id, 5000, a.created_at.clone()));
        store.upsert_dataset(dataset("other", 10)).await.unwrap();
        assert_eq!(store.datasets().await.unwrap().len(), 2);
        assert_eq!(store.dataset_by_name("dfrc-1").await.unwrap().map(|d| d.id), Some(a.id));
        store.delete_dataset(a.id).await.unwrap();
        assert!(store.dataset(a.id).await.unwrap().is_none());
        assert_eq!(store.delete_dataset(a.id).await.unwrap_err().status(), 404);
    }

    #[tokio::test]
    async fn nets_are_known_by_their_hash_and_builds_by_their_net() {
        let (store, _dir) = testing::store();
        let shipped = store.insert_net(net("v60-7f587dfb", "7f58")).await.unwrap();
        let again = store.insert_net(net("other-name", "7f58")).await.unwrap();
        assert_eq!(again.id, shipped.id);
        assert_eq!(store.insert_net(net("v60-7f587dfb", "beef")).await.unwrap_err().status(), 409);
        let d = store.upsert_dataset(dataset("dfrc-1", 1000)).await.unwrap();
        let trained = store
            .insert_net(NewNet {
                parent: Some(shipped.id),
                dataset: Some(d.id),
                run: Some(NetRun {
                    idea: Some(IdeaId(3)),
                    work: Some(WorkId(9)),
                    superbatches: 20,
                    positions: 65_536_000,
                    final_loss: Some(0.0012),
                }),
                ..net("net-9", "abcd")
            })
            .await
            .unwrap();
        assert_eq!(trained.run.as_ref().map(|r| r.superbatches), Some(20));
        assert_eq!(store.nets().await.unwrap().len(), 2);
        assert_eq!(store.net_by_sha("ABCD").await.unwrap().map(|n| n.id), Some(trained.id));
        // Deleting the dataset keeps the net, without the link.
        store.delete_dataset(d.id).await.unwrap();
        assert_eq!(store.net(trained.id).await.unwrap().unwrap().dataset, None);

        // A version can be code plus a net; builds are cached per net.
        let mut v = NewVersion::new("v1 + net-9", "aaa", Line::Other);
        v.net = Some(trained.id);
        let v = store.upsert_version(v).await.unwrap();
        assert_eq!(v.net, Some(trained.id));
        for net in [None, Some(trained.id)] {
            let b = store
                .insert_build(NewBuild {
                    version: v.id,
                    commit: "aaa".into(),
                    features: vec![],
                    net,
                    path: "x".into(),
                    status: BuildStatus::Building,
                    log_path: None,
                })
                .await
                .unwrap();
            store.update_build(b.id, BuildStatus::Ready, Some(1), None).await.unwrap();
        }
        let own = store.cached_build("aaa", &[]).await.unwrap().unwrap();
        let with = store.cached_build_with_net("aaa", &[], Some(trained.id)).await.unwrap().unwrap();
        assert_eq!((own.net, with.net), (None, Some(trained.id)));
        assert!(store.cached_build_with_net("aaa", &[], Some(shipped.id)).await.unwrap().is_none());
    }

    #[tokio::test]
    async fn train_steps_carry_on_across_runs() {
        let (store, _dir) = testing::store();
        let step = |sb: u32, loss: f64| TrainStep {
            seq: 0,
            superbatch: sb,
            batch: 200,
            loss,
            lr: 0.001,
            grad_norm: Some(0.5),
            positions_per_sec: 700_000.0,
            elapsed_seconds: 1.0,
        };
        store.add_train_steps(WorkId(1), Some(IdeaId(2)), vec![step(1, 0.1), step(2, 0.05)]).await.unwrap();
        store.add_train_steps(WorkId(1), Some(IdeaId(2)), vec![step(3, 0.04)]).await.unwrap();
        store.add_train_steps(WorkId(5), Some(IdeaId(2)), vec![step(4, 0.03)]).await.unwrap();
        let w1 = store.train_steps(WorkId(1)).await.unwrap();
        assert_eq!(w1.iter().map(|s| s.seq).collect::<Vec<_>>(), [1, 2, 3]);
        let all = store.idea_train_steps(IdeaId(2)).await.unwrap();
        assert_eq!(all.iter().map(|s| (s.seq, s.superbatch)).collect::<Vec<_>>(), [(1, 1), (2, 2), (3, 3), (4, 4)]);
    }
}
