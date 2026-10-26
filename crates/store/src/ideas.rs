//! Ideas, their trials, and a tune's SPSA steps. An idea's `spec` (its funnel options, a tune's settings) is JSON the
//! `work` crate owns; the store keeps it and gives it back.

use dameless_core::{
    Error, IdeaId, Timestamp, TrialId, VersionId,
    ideas::{Author, Idea, IdeaKind, IdeaStatus, Preset, Stage, Trial, TrialStatus},
    spsa::TuneStep,
    stats::Pentanomial,
    time::TimeControl,
};
use rusqlite::{Connection, OptionalExtension, Row, params};

use crate::{DbResult, Store, from_json, from_opt_json, from_opt_word, from_word, now, opt_json, to_json, word};

/// An idea to queue.
#[derive(Clone, Debug, PartialEq)]
pub struct NewIdea {
    pub title: String,
    pub hypothesis: String,
    pub kind: IdeaKind,
    pub author: Author,
    pub branch: String,
    pub base: VersionId,
    pub tags: Vec<String>,
    pub notes: Option<String>,
    /// What the work crate needs to run it: the funnel's options, a tune's settings, a custom trial's.
    pub spec: serde_json::Value,
}

/// Changes to an idea. Fields left `None` stay as they are.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct IdeaUpdate {
    pub status: Option<IdeaStatus>,
    pub verdict: Option<String>,
    pub version: Option<VersionId>,
    pub notes: Option<String>,
    pub tags: Option<Vec<String>>,
    pub ended_at: Option<Timestamp>,
}

/// Which ideas a list holds: empty or `None` for all.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct IdeaFilter {
    pub statuses: Vec<IdeaStatus>,
    /// Ideas with this tag (a batch).
    pub tag: Option<String>,
    /// At most this many, newest first; 0 for all.
    pub limit: u32,
}

/// A trial to add to an idea, after its others.
#[derive(Clone, Debug, PartialEq)]
pub struct NewTrial {
    pub stage: Stage,
    pub preset: Option<Preset>,
    pub time: Option<TimeControl>,
    pub bounds: Option<[f64; 2]>,
    pub pairs: Option<u32>,
}

const IDEA: &str = "SELECT id, title, hypothesis, kind, author, branch, base, version, status, verdict, tags, notes, \
                    created_at, ended_at FROM ideas";

fn idea_from_row(row: &Row<'_>) -> DbResult<Idea> {
    Ok(Idea {
        id: IdeaId(row.get(0)?),
        title: row.get(1)?,
        hypothesis: row.get(2)?,
        kind: from_word(&row.get::<_, String>(3)?)?,
        author: from_word(&row.get::<_, String>(4)?)?,
        branch: row.get(5)?,
        base: VersionId(row.get(6)?),
        version: row.get::<_, Option<i64>>(7)?.map(VersionId),
        status: from_word(&row.get::<_, String>(8)?)?,
        verdict: row.get(9)?,
        tags: from_json(&row.get::<_, String>(10)?)?,
        notes: row.get(11)?,
        created_at: Timestamp(row.get(12)?),
        ended_at: row.get::<_, Option<String>>(13)?.map(Timestamp),
        trials: Vec::new(),
    })
}

const TRIAL: &str = "SELECT id, idea, stage, status, preset, time, played, scaling, bounds, pairs, penta, wdl, llr, \
                     elo, nelo, detail, started_at, ended_at FROM trials";

fn trial_from_row(row: &Row<'_>) -> DbResult<Trial> {
    Ok(Trial {
        id: TrialId(row.get(0)?),
        idea: IdeaId(row.get(1)?),
        stage: from_word(&row.get::<_, String>(2)?)?,
        status: from_word(&row.get::<_, String>(3)?)?,
        preset: from_opt_word(row.get(4)?)?,
        time: from_opt_json(row.get(5)?)?,
        played: from_opt_json(row.get(6)?)?,
        scaling: from_opt_json(row.get(7)?)?,
        bounds: from_opt_json(row.get(8)?)?,
        pairs: row.get(9)?,
        penta: from_json(&row.get::<_, String>(10)?)?,
        wdl: from_json(&row.get::<_, String>(11)?)?,
        llr: from_opt_json(row.get(12)?)?,
        elo: from_opt_json(row.get(13)?)?,
        nelo: from_opt_json(row.get(14)?)?,
        detail: row.get(15)?,
        started_at: row.get::<_, Option<String>>(16)?.map(Timestamp),
        ended_at: row.get::<_, Option<String>>(17)?.map(Timestamp),
    })
}

fn trials_of(conn: &Connection, idea: IdeaId) -> DbResult<Vec<Trial>> {
    let mut stmt = conn.prepare_cached(&format!("{TRIAL} WHERE idea = ?1 ORDER BY ord"))?;
    let mut rows = stmt.query([idea.0])?;
    let mut out = Vec::new();
    while let Some(row) = rows.next()? {
        out.push(trial_from_row(row)?);
    }
    Ok(out)
}

fn trial_by_id(conn: &Connection, id: TrialId) -> DbResult<Option<Trial>> {
    let mut stmt = conn.prepare_cached(&format!("{TRIAL} WHERE id = ?1"))?;
    let mut rows = stmt.query([id.0])?;
    rows.next()?.map(trial_from_row).transpose()
}

/// An idea with its trials' summaries.
fn idea_by_id(conn: &Connection, id: IdeaId) -> DbResult<Option<Idea>> {
    let mut stmt = conn.prepare_cached(&format!("{IDEA} WHERE id = ?1"))?;
    let mut rows = stmt.query([id.0])?;
    let Some(mut idea) = rows.next()?.map(idea_from_row).transpose()? else { return Ok(None) };
    idea.trials = trials_of(conn, id)?.iter().map(Trial::summary).collect();
    Ok(Some(idea))
}

fn idea_or_missing(conn: &Connection, id: IdeaId) -> DbResult<Idea> {
    idea_by_id(conn, id)?.ok_or_else(|| Error::not_found(format!("No idea {id}")).into())
}

impl Store {
    /// Queues an idea; returns it, queued and without trials.
    pub async fn insert_idea(&self, idea: NewIdea) -> Result<Idea, Error> {
        self.write(move |tx| {
            tx.execute(
                "INSERT INTO ideas (title, hypothesis, kind, author, branch, base, status, tags, notes, spec, created_at) \
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11)",
                params![
                    idea.title,
                    idea.hypothesis,
                    word(&idea.kind),
                    word(&idea.author),
                    idea.branch,
                    idea.base.0,
                    word(&IdeaStatus::Queued),
                    to_json(&idea.tags)?,
                    idea.notes,
                    to_json(&idea.spec)?,
                    now().as_str(),
                ],
            )?;
            idea_or_missing(tx, IdeaId(tx.last_insert_rowid()))
        })
        .await
    }

    /// An idea with its trials' summaries.
    pub async fn idea(&self, id: IdeaId) -> Result<Option<Idea>, Error> {
        self.read(move |conn| idea_by_id(conn, id)).await
    }

    /// The JSON the idea was queued with.
    pub async fn idea_spec(&self, id: IdeaId) -> Result<Option<serde_json::Value>, Error> {
        self.read(move |conn| {
            let text: Option<String> =
                conn.query_row("SELECT spec FROM ideas WHERE id = ?1", [id.0], |r| r.get(0)).optional()?;
            text.as_deref().map(from_json).transpose()
        })
        .await
    }

    /// Ideas, newest first, each with its trials' summaries.
    pub async fn ideas(&self, filter: IdeaFilter) -> Result<Vec<Idea>, Error> {
        self.read(move |conn| {
            let statuses: Vec<String> = filter.statuses.iter().map(word).collect();
            let mut stmt = conn.prepare_cached(&format!(
                "{IDEA} WHERE (?1 = '[]' OR status IN (SELECT value FROM json_each(?1))) \
                 AND (?2 IS NULL OR EXISTS (SELECT 1 FROM json_each(ideas.tags) WHERE value = ?2)) \
                 ORDER BY id DESC LIMIT ?3"
            ))?;
            let limit = if filter.limit == 0 { -1 } else { i64::from(filter.limit) };
            let mut rows = stmt.query(params![to_json(&statuses)?, filter.tag, limit])?;
            let mut out = Vec::new();
            while let Some(row) = rows.next()? {
                out.push(idea_from_row(row)?);
            }
            for idea in &mut out {
                idea.trials = trials_of(conn, idea.id)?.iter().map(Trial::summary).collect();
            }
            Ok(out)
        })
        .await
    }

    /// Changes an idea; returns it as it now is.
    pub async fn update_idea(&self, id: IdeaId, u: IdeaUpdate) -> Result<Idea, Error> {
        self.write(move |tx| {
            idea_or_missing(tx, id)?;
            tx.execute(
                "UPDATE ideas SET status = coalesce(?2, status), verdict = coalesce(?3, verdict), \
                 version = coalesce(?4, version), notes = coalesce(?5, notes), tags = coalesce(?6, tags), \
                 ended_at = coalesce(?7, ended_at) WHERE id = ?1",
                params![
                    id.0,
                    u.status.map(|s| word(&s)),
                    u.verdict,
                    u.version.map(|v| v.0),
                    u.notes,
                    opt_json(&u.tags)?,
                    u.ended_at.as_ref().map(Timestamp::as_str),
                ],
            )?;
            idea_or_missing(tx, id)
        })
        .await
    }

    /// Adds a trial after the idea's others; returns it, waiting, with no games.
    pub async fn insert_trial(&self, idea: IdeaId, t: NewTrial) -> Result<Trial, Error> {
        self.write(move |tx| {
            idea_or_missing(tx, idea)?;
            let ord: i64 =
                tx.query_row("SELECT coalesce(max(ord), -1) + 1 FROM trials WHERE idea = ?1", [idea.0], |r| r.get(0))?;
            tx.execute(
                "INSERT INTO trials (idea, stage, ord, status, preset, time, bounds, pairs, penta, wdl) \
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10)",
                params![
                    idea.0,
                    word(&t.stage),
                    ord,
                    word(&TrialStatus::Waiting),
                    t.preset.map(|p| word(&p)),
                    opt_json(&t.time)?,
                    opt_json(&t.bounds)?,
                    t.pairs,
                    to_json(&Pentanomial::default())?,
                    to_json(&[0u32; 3])?,
                ],
            )?;
            trial_by_id(tx, TrialId(tx.last_insert_rowid()))?.ok_or_else(|| Error::internal("a trial vanished").into())
        })
        .await
    }

    pub async fn trial(&self, id: TrialId) -> Result<Option<Trial>, Error> {
        self.read(move |conn| trial_by_id(conn, id)).await
    }

    /// An idea's trials in funnel order.
    pub async fn trials(&self, idea: IdeaId) -> Result<Vec<Trial>, Error> {
        self.read(move |conn| trials_of(conn, idea)).await
    }

    /// Saves a trial's state and statistics as the work running it holds them (its id, idea and stage stay).
    pub async fn save_trial(&self, t: Trial) -> Result<(), Error> {
        self.write(move |tx| {
            let n = tx.execute(
                "UPDATE trials SET status = ?2, preset = ?3, time = ?4, played = ?5, scaling = ?6, bounds = ?7, \
                 pairs = ?8, penta = ?9, wdl = ?10, llr = ?11, elo = ?12, nelo = ?13, detail = ?14, started_at = ?15, \
                 ended_at = ?16 WHERE id = ?1",
                params![
                    t.id.0,
                    word(&t.status),
                    t.preset.map(|p| word(&p)),
                    opt_json(&t.time)?,
                    opt_json(&t.played)?,
                    opt_json(&t.scaling)?,
                    opt_json(&t.bounds)?,
                    t.pairs,
                    to_json(&t.penta)?,
                    to_json(&t.wdl)?,
                    opt_json(&t.llr)?,
                    opt_json(&t.elo)?,
                    opt_json(&t.nelo)?,
                    t.detail,
                    t.started_at.as_ref().map(Timestamp::as_str),
                    t.ended_at.as_ref().map(Timestamp::as_str),
                ],
            )?;
            if n == 0 { Err(Error::not_found(format!("No trial {}", t.id)).into()) } else { Ok(()) }
        })
        .await
    }

    /// Records a finished round: how many rounds the trial has played and where its PGN is.
    pub async fn set_trial_rounds(&self, id: TrialId, rounds: u32, pgn_path: Option<String>) -> Result<(), Error> {
        self.write(move |tx| {
            let n = tx.execute(
                "UPDATE trials SET rounds = ?2, pgn_path = coalesce(?3, pgn_path) WHERE id = ?1",
                params![id.0, rounds, pgn_path],
            )?;
            if n == 0 { Err(Error::not_found(format!("No trial {id}")).into()) } else { Ok(()) }
        })
        .await
    }

    /// How many rounds a trial has played and where its PGN is: where a resumed trial carries on from.
    pub async fn trial_rounds(&self, id: TrialId) -> Result<Option<(u32, Option<String>)>, Error> {
        self.read(move |conn| {
            Ok(conn
                .query_row("SELECT rounds, pgn_path FROM trials WHERE id = ?1", [id.0], |r| Ok((r.get(0)?, r.get(1)?)))
                .optional()?)
        })
        .await
    }

    /// Appends an SPSA update to a tune.
    pub async fn append_tune_step(&self, idea: IdeaId, step: TuneStep) -> Result<(), Error> {
        self.write(move |tx| {
            tx.execute(
                "INSERT INTO tune_steps (idea, iteration, theta, result, penta) VALUES (?1, ?2, ?3, ?4, ?5)",
                params![idea.0, step.iteration, to_json(&step.values)?, step.result, to_json(&step.penta)?],
            )?;
            Ok(())
        })
        .await
    }

    /// A tune's updates in the order they were applied.
    pub async fn tune_steps(&self, idea: IdeaId) -> Result<Vec<TuneStep>, Error> {
        self.read(move |conn| {
            let mut stmt = conn.prepare_cached(
                "SELECT iteration, theta, result, penta FROM tune_steps WHERE idea = ?1 ORDER BY iteration",
            )?;
            let mut rows = stmt.query([idea.0])?;
            let mut out = Vec::new();
            while let Some(row) = rows.next()? {
                out.push(TuneStep {
                    iteration: row.get(0)?,
                    values: from_json(&row.get::<_, String>(1)?)?,
                    result: row.get(2)?,
                    penta: from_json(&row.get::<_, String>(3)?)?,
                });
            }
            Ok(out)
        })
        .await
    }
}

#[cfg(test)]
mod tests {
    use dameless_core::{
        PlayerKey,
        game::Source,
        version::{Estimate, Line, Llr},
    };

    use super::*;
    use crate::{GameFilter, NewVersion, games::tests::local, testing};

    async fn idea(store: &Store, tags: &[&str]) -> Idea {
        let base = store.upsert_version(NewVersion::new("v1", "aaa", Line::Champion { number: 1 })).await.unwrap();
        store
            .insert_idea(NewIdea {
                title: "Deeper LMR".into(),
                hypothesis: "Reduce more at high depth".into(),
                kind: IdeaKind::Gain,
                author: Author::Claude,
                branch: "deeper-lmr".into(),
                base: base.id,
                tags: tags.iter().map(|s| s.to_string()).collect(),
                notes: None,
                spec: serde_json::json!({ "long": false }),
            })
            .await
            .unwrap()
    }

    #[tokio::test]
    async fn ideas_carry_their_trials() {
        let (store, _dir) = testing::store();
        let i = idea(&store, &["batch-1"]).await;
        assert_eq!((i.status, i.trials.len()), (IdeaStatus::Queued, 0));
        assert_eq!(store.idea_spec(i.id).await.unwrap(), Some(serde_json::json!({ "long": false })));

        let fp = store
            .insert_trial(
                i.id,
                NewTrial { stage: Stage::Fingerprint, preset: None, time: None, bounds: None, pairs: None },
            )
            .await
            .unwrap();
        let mut short = store
            .insert_trial(
                i.id,
                NewTrial {
                    stage: Stage::Short,
                    preset: Some(Preset::Stc),
                    time: Some(Preset::Stc.settings().time),
                    bounds: Some([0.0, 3.0]),
                    pairs: None,
                },
            )
            .await
            .unwrap();
        assert_eq!(short.status, TrialStatus::Waiting);

        short.status = TrialStatus::Running;
        short.penta = Pentanomial([1, 2, 3, 4, 5]);
        short.wdl = [10, 10, 10];
        short.llr = Some(Llr { value: 1.0, lower: -2.25, upper: 2.89 });
        short.elo = Some(Estimate::plus_minus(5.0, 3.0));
        store.save_trial(short.clone()).await.unwrap();
        assert_eq!(store.trial(short.id).await.unwrap(), Some(short.clone()));
        store.set_trial_rounds(short.id, 3, Some("games/trial-2.pgn".into())).await.unwrap();
        assert_eq!(store.trial_rounds(short.id).await.unwrap(), Some((3, Some("games/trial-2.pgn".into()))));

        let got = store.idea(i.id).await.unwrap().unwrap();
        assert_eq!(got.trials.iter().map(|t| t.id).collect::<Vec<_>>(), [fp.id, short.id]);
        assert_eq!(got.trials[1].pairs, 15);

        let updated = store
            .update_idea(
                i.id,
                IdeaUpdate { status: Some(IdeaStatus::Kept), verdict: Some("Kept".into()), ..Default::default() },
            )
            .await
            .unwrap();
        assert_eq!((updated.status, updated.verdict.as_deref()), (IdeaStatus::Kept, Some("Kept")));
        assert_eq!(updated.title, "Deeper LMR");

        // Trial games are counted and listed by trial.
        let mut g = local(PlayerKey::Dameless, PlayerKey::Dameless, 0);
        g.source = Source::Trial { trial: short.id, index: 0 };
        store.insert_games(vec![g.clone(), g]).await.unwrap();
        assert_eq!(store.count_trial_games(short.id).await.unwrap(), 2);
        let listed = store.games(GameFilter { trial: Some(short.id), ..Default::default() }).await.unwrap();
        assert_eq!(listed.len(), 2);
    }

    #[tokio::test]
    async fn ideas_list_by_status_and_tag() {
        let (store, _dir) = testing::store();
        let a = idea(&store, &["batch-1"]).await;
        let b = idea(&store, &["batch-2", "lmr"]).await;
        store.update_idea(b.id, IdeaUpdate { status: Some(IdeaStatus::Running), ..Default::default() }).await.unwrap();
        let ids = |v: Vec<Idea>| v.into_iter().map(|i| i.id).collect::<Vec<_>>();
        assert_eq!(ids(store.ideas(IdeaFilter::default()).await.unwrap()), [b.id, a.id]);
        let running =
            store.ideas(IdeaFilter { statuses: vec![IdeaStatus::Running], ..Default::default() }).await.unwrap();
        assert_eq!(ids(running), [b.id]);
        let tagged = store.ideas(IdeaFilter { tag: Some("lmr".into()), ..Default::default() }).await.unwrap();
        assert_eq!(ids(tagged), [b.id]);
        let one = store.ideas(IdeaFilter { limit: 1, ..Default::default() }).await.unwrap();
        assert_eq!(ids(one), [b.id]);
        assert_eq!(store.update_idea(IdeaId(99), IdeaUpdate::default()).await.unwrap_err().status(), 404);
    }

    #[tokio::test]
    async fn tune_steps_append_in_order() {
        let (store, _dir) = testing::store();
        let i = idea(&store, &[]).await;
        for n in [1, 2, 3] {
            store
                .append_tune_step(
                    i.id,
                    TuneStep {
                        iteration: n,
                        values: vec![n as f64, 2.5],
                        result: 1,
                        penta: Pentanomial([0, 0, 1, 1, 0]),
                    },
                )
                .await
                .unwrap();
        }
        let steps = store.tune_steps(i.id).await.unwrap();
        assert_eq!(steps.iter().map(|s| s.iteration).collect::<Vec<_>>(), [1, 2, 3]);
        assert_eq!(steps[2].values, [3.0, 2.5]);
        // An iteration is applied once.
        let dup = TuneStep { iteration: 2, values: vec![], result: 0, penta: Pentanomial::default() };
        assert!(store.append_tune_step(i.id, dup).await.is_err());
    }
}
