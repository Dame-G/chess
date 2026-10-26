-- R4, training (docs/roadmap.md): datasets, nets, the readings of training runs, and the net a version and a build
-- embed.

-- A dataset directory under the data directory's `data/`, read from its manifest.json. `source` is core's
-- DatasetSource as JSON; `source_kind` (datagen|import|trials|other) is copied out of it.
CREATE TABLE datasets (
    id          INTEGER PRIMARY KEY,
    name        TEXT NOT NULL UNIQUE,
    format      TEXT NOT NULL,
    positions   INTEGER NOT NULL,
    games       INTEGER NOT NULL,
    size_bytes  INTEGER NOT NULL,
    source      TEXT NOT NULL,
    source_kind TEXT NOT NULL,
    score_units TEXT,
    path        TEXT NOT NULL,
    created_at  TEXT NOT NULL,
    updated_at  TEXT NOT NULL
) STRICT;

-- A net file under the data directory's `nets/`, by its SHA-256. `run` is core's NetRun as JSON.
CREATE TABLE nets (
    id         INTEGER PRIMARY KEY,
    name       TEXT NOT NULL UNIQUE,
    sha256     TEXT NOT NULL UNIQUE,
    path       TEXT NOT NULL,
    arch       TEXT NOT NULL,
    size_bytes INTEGER NOT NULL,
    parent     INTEGER REFERENCES nets (id),
    dataset    INTEGER REFERENCES datasets (id) ON DELETE SET NULL,
    run        TEXT,
    metrics    TEXT,
    created_at TEXT NOT NULL
) STRICT;

-- Every progress reading of a training run, for its loss chart. `idea` is the net idea it ran for, if any.
CREATE TABLE train_steps (
    work              INTEGER NOT NULL,
    seq               INTEGER NOT NULL,
    idea              INTEGER,
    superbatch        INTEGER NOT NULL,
    batch             INTEGER NOT NULL,
    loss              REAL NOT NULL,
    lr                REAL NOT NULL,
    grad_norm         REAL,
    positions_per_sec REAL NOT NULL,
    elapsed_seconds   REAL NOT NULL,
    PRIMARY KEY (work, seq)
) STRICT;
CREATE INDEX train_steps_idea ON train_steps (idea, work, seq);

-- The net a version's builds embed (NULL: the commit's own), and a build's. The build cache's key becomes
-- (commit_sha, features, net).
ALTER TABLE versions ADD COLUMN net INTEGER REFERENCES nets (id);
ALTER TABLE builds ADD COLUMN net INTEGER REFERENCES nets (id);
DROP INDEX builds_cache;
CREATE INDEX builds_cache ON builds (commit_sha, features, net);
