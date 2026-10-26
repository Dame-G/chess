-- The first schema: docs/design.md, "Storage". Timestamps are ISO-8601 UTC text; JSON columns hold structured values
-- the database never filters on; the columns it does filter on are plain words and numbers beside them.

CREATE TABLE meta (
    key   TEXT PRIMARY KEY,
    value TEXT NOT NULL
) STRICT;

CREATE TABLE players (
    key        TEXT PRIMARY KEY,
    name       TEXT NOT NULL,
    title      TEXT,
    lichess    TEXT,
    ratings    TEXT NOT NULL,
    updated_at TEXT NOT NULL
) STRICT;

CREATE TABLE versions (
    id         INTEGER PRIMARY KEY,
    name       TEXT NOT NULL UNIQUE,
    commit_sha TEXT NOT NULL,
    branch     TEXT,
    line       TEXT NOT NULL,
    line_kind  TEXT NOT NULL,
    champion   INTEGER,
    parent     INTEGER REFERENCES versions (id),
    bench      INTEGER,
    elo        TEXT,
    created_at TEXT NOT NULL
) STRICT;
CREATE INDEX versions_commit ON versions (commit_sha);
CREATE INDEX versions_line ON versions (line_kind);
CREATE UNIQUE INDEX versions_champion ON versions (champion) WHERE champion IS NOT NULL;

CREATE TABLE builds (
    id         INTEGER PRIMARY KEY,
    version    INTEGER NOT NULL REFERENCES versions (id),
    commit_sha TEXT NOT NULL,
    features   TEXT NOT NULL,
    bench      INTEGER,
    nps        INTEGER,
    path       TEXT NOT NULL,
    status     TEXT NOT NULL,
    log_path   TEXT,
    created_at TEXT NOT NULL,
    deleted_at TEXT
) STRICT;
CREATE INDEX builds_cache ON builds (commit_sha, features);
CREATE INDEX builds_version ON builds (version);

CREATE TABLE ideas (
    id         INTEGER PRIMARY KEY,
    title      TEXT NOT NULL,
    hypothesis TEXT NOT NULL,
    kind       TEXT NOT NULL,
    author     TEXT NOT NULL,
    branch     TEXT NOT NULL,
    base       INTEGER NOT NULL REFERENCES versions (id),
    version    INTEGER REFERENCES versions (id),
    status     TEXT NOT NULL,
    verdict    TEXT,
    tags       TEXT NOT NULL,
    notes      TEXT,
    spec       TEXT NOT NULL,
    created_at TEXT NOT NULL,
    ended_at   TEXT
) STRICT;
CREATE INDEX ideas_status ON ideas (status);

CREATE TABLE trials (
    id         INTEGER PRIMARY KEY,
    idea       INTEGER NOT NULL REFERENCES ideas (id) ON DELETE CASCADE,
    stage      TEXT NOT NULL,
    ord        INTEGER NOT NULL,
    status     TEXT NOT NULL,
    preset     TEXT,
    time       TEXT,
    played     TEXT,
    scaling    TEXT,
    bounds     TEXT,
    pairs      INTEGER,
    penta      TEXT NOT NULL,
    wdl        TEXT NOT NULL,
    llr        TEXT,
    elo        TEXT,
    nelo       TEXT,
    detail     TEXT,
    rounds     INTEGER NOT NULL DEFAULT 0,
    pgn_path   TEXT,
    started_at TEXT,
    ended_at   TEXT,
    UNIQUE (idea, ord)
) STRICT;

CREATE TABLE tune_steps (
    idea      INTEGER NOT NULL REFERENCES ideas (id) ON DELETE CASCADE,
    iteration INTEGER NOT NULL,
    theta     TEXT NOT NULL,
    result    INTEGER NOT NULL,
    penta     TEXT NOT NULL,
    PRIMARY KEY (idea, iteration)
) STRICT, WITHOUT ROWID;

CREATE TABLE games (
    id           INTEGER PRIMARY KEY,
    source_kind  TEXT NOT NULL,
    source       TEXT NOT NULL,
    source_owner TEXT,
    white        TEXT NOT NULL,
    black        TEXT NOT NULL,
    white_key    TEXT NOT NULL,
    black_key    TEXT NOT NULL,
    time         TEXT NOT NULL,
    speed        TEXT,
    rated        INTEGER NOT NULL,
    status       TEXT NOT NULL,
    live         INTEGER NOT NULL,
    outcome      TEXT,
    ending       TEXT,
    opening      TEXT,
    start_fen    TEXT,
    plies        INTEGER NOT NULL,
    moves        BLOB NOT NULL,
    you_side     TEXT,
    lichess_id   TEXT,
    trial        INTEGER REFERENCES trials (id) ON DELETE CASCADE,
    url          TEXT,
    started_at   TEXT NOT NULL,
    ended_at     TEXT
) STRICT;
CREATE INDEX games_newest ON games (live, started_at, id);
CREATE INDEX games_white ON games (white_key, started_at);
CREATE INDEX games_black ON games (black_key, started_at);
CREATE INDEX games_source ON games (source_kind, started_at);
CREATE INDEX games_trial ON games (trial);
CREATE INDEX games_lichess ON games (lichess_id, source_kind, source_owner);

CREATE TABLE reviews (
    id             INTEGER PRIMARY KEY,
    game           INTEGER NOT NULL REFERENCES games (id) ON DELETE CASCADE,
    version        INTEGER NOT NULL REFERENCES versions (id),
    nodes          INTEGER NOT NULL,
    white_accuracy REAL,
    black_accuracy REAL,
    summary        TEXT NOT NULL,
    plies          BLOB NOT NULL,
    created_at     TEXT NOT NULL,
    UNIQUE (game, version, nodes)
) STRICT;
CREATE INDEX reviews_newest ON reviews (game, created_at, id);

CREATE TABLE positions (
    key        TEXT NOT NULL,
    commit_sha TEXT NOT NULL,
    nodes      INTEGER NOT NULL,
    multipv    INTEGER NOT NULL,
    result     TEXT NOT NULL,
    PRIMARY KEY (key, commit_sha, nodes, multipv)
) STRICT, WITHOUT ROWID;

CREATE TABLE work (
    id         INTEGER PRIMARY KEY,
    kind       TEXT NOT NULL,
    status     TEXT NOT NULL,
    priority   INTEGER NOT NULL,
    title      TEXT NOT NULL,
    subject    TEXT NOT NULL,
    spec       TEXT NOT NULL,
    checkpoint TEXT,
    progress   TEXT,
    cores      INTEGER NOT NULL DEFAULT 0,
    paused_for TEXT,
    error      TEXT,
    created_at TEXT NOT NULL,
    started_at TEXT,
    ended_at   TEXT
) STRICT;
CREATE INDEX work_status ON work (status, priority, id);

CREATE TABLE connections (
    key        TEXT PRIMARY KEY,
    account    TEXT,
    settings   TEXT,
    state      TEXT,
    updated_at TEXT NOT NULL
) STRICT;
