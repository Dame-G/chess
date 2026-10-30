-- The engines list replaces Dameless's versions: any UCI engine can be an opponent or the analysis engine. Ideas,
-- trials, tunes, versions, builds, nets, datasets and training go, and so does the Lichess bot. Migrations run with
-- foreign keys off (migrate.rs), so the rows that pointed at what is dropped are deleted here by hand.

-- The engines you added, and the built-ins once installed. A built-in that is not installed has no row.
CREATE TABLE engines (
    id         TEXT PRIMARY KEY,
    name       TEXT NOT NULL,
    kind       TEXT NOT NULL,
    path       TEXT NOT NULL,
    args       TEXT NOT NULL,
    options    TEXT NOT NULL,
    id_name    TEXT,
    nodes      INTEGER,
    rating     INTEGER,
    created_at TEXT NOT NULL
) STRICT;

-- What reviewed an old game was a commit of Reckless: its label from here on, and its key in the reviews table.
CREATE TEMP TABLE old_labels AS
    SELECT id, 'Reckless ' || substr(commit_sha, 1, 7) AS label FROM versions;

-- Trial games go, with everything that hangs off them.
DELETE FROM reviews WHERE game IN (SELECT id FROM games WHERE source_kind = 'trial');
DELETE FROM drills WHERE game IN (SELECT id FROM games WHERE source_kind = 'trial');
DELETE FROM level_games WHERE game IN (SELECT id FROM games WHERE source_kind = 'trial');
DELETE FROM games WHERE source_kind = 'trial';

-- The levels were Reckless at node budgets. Calibration games between them go (they were never shown), and so do the
-- ratings and the record of who sat at a level: the ladder is now the analysis engine's and calibrates afresh. Your games
-- against those levels stay, as games against Reckless.
CREATE TEMP TABLE calibration_games AS SELECT game FROM level_games WHERE calibration = 1;
DELETE FROM reviews WHERE game IN (SELECT game FROM calibration_games);
DELETE FROM drills WHERE game IN (SELECT game FROM calibration_games);
DELETE FROM games WHERE id IN (SELECT game FROM calibration_games);
DELETE FROM level_games;
DELETE FROM level_ratings;
DROP TABLE calibration_games;

-- Dameless's seats become Reckless's (`engine:reckless`), named by the engine and labelled with the commit they played.
UPDATE games SET white = json_set(json_remove(white, '$.version', '$.build'),
        '$.player', 'engine:reckless',
        '$.name', replace(json_extract(white, '$.name'), 'Dameless', 'Reckless'),
        '$.engine', (SELECT label FROM old_labels WHERE id = json_extract(white, '$.version'))),
    white_key = 'engine:reckless'
    WHERE white_key = 'dameless';
UPDATE games SET black = json_set(json_remove(black, '$.version', '$.build'),
        '$.player', 'engine:reckless',
        '$.name', replace(json_extract(black, '$.name'), 'Dameless', 'Reckless'),
        '$.engine', (SELECT label FROM old_labels WHERE id = json_extract(black, '$.version'))),
    black_key = 'engine:reckless'
    WHERE black_key = 'dameless';
UPDATE games SET white = json_remove(white, '$.version', '$.build'), black = json_remove(black, '$.version', '$.build');

-- Lichess's AI (`stockfish:<n>`) folds into the Stockfish engine, its level kept on the seat.
UPDATE games SET white = json_set(white, '$.player', 'engine:stockfish',
        '$.level', CAST(substr(white_key, 11) AS INTEGER)),
    white_key = 'engine:stockfish'
    WHERE white_key LIKE 'stockfish:%';
UPDATE games SET black = json_set(black, '$.player', 'engine:stockfish',
        '$.level', CAST(substr(black_key, 11) AS INTEGER)),
    black_key = 'engine:stockfish'
    WHERE black_key LIKE 'stockfish:%';

-- The bot's Lichess games stay as Reckless's history on Lichess: imported games owned by `engine:reckless`.
UPDATE games SET
    source = json_object('type', 'import', 'owner', 'engine:reckless', 'id', json_extract(source, '$.id'),
        'rated', json(CASE WHEN json_extract(source, '$.rated') THEN 'true' ELSE 'false' END),
        'speed', json_extract(source, '$.speed')),
    source_kind = 'import',
    source_owner = 'engine:reckless'
    WHERE source_kind = 'lichess' AND json_extract(source, '$.connection') = 'bot';

DELETE FROM players WHERE key = 'dameless' OR key LIKE 'stockfish:%';

-- Games lose their trial column, which pointed at the trials table.
CREATE TABLE games_new (
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
    url          TEXT,
    started_at   TEXT NOT NULL,
    ended_at     TEXT
) STRICT;
INSERT INTO games_new
    SELECT id, source_kind, source, source_owner, white, black, white_key, black_key, time, speed, rated, status, live,
           outcome, ending, opening, start_fen, plies, moves, you_side, lichess_id, url, started_at, ended_at
    FROM games;
DROP TABLE games;
ALTER TABLE games_new RENAME TO games;
CREATE INDEX games_newest ON games (live, started_at, id);
CREATE INDEX games_white ON games (white_key, started_at);
CREATE INDEX games_black ON games (black_key, started_at);
CREATE INDEX games_source ON games (source_kind, started_at);
CREATE INDEX games_lichess ON games (lichess_id, source_kind, source_owner);

-- Reviews are keyed by the engine that made them (`<engine id>|<what it calls itself>`) instead of a version, and their
-- summaries name it.
CREATE TABLE reviews_new (
    id             INTEGER PRIMARY KEY,
    game           INTEGER NOT NULL REFERENCES games (id) ON DELETE CASCADE,
    engine         TEXT NOT NULL,
    nodes          INTEGER NOT NULL,
    white_accuracy REAL,
    black_accuracy REAL,
    summary        TEXT NOT NULL,
    plies          BLOB NOT NULL,
    created_at     TEXT NOT NULL,
    UNIQUE (game, engine, nodes)
) STRICT;
INSERT INTO reviews_new
    SELECT r.id, r.game, 'reckless|' || coalesce(l.label, 'Reckless'), r.nodes, r.white_accuracy, r.black_accuracy,
           json_set(json_remove(r.summary, '$.version'), '$.engine', coalesce(l.label, 'Reckless')), r.plies,
           r.created_at
    FROM reviews r LEFT JOIN old_labels l ON l.id = r.version;
DROP TABLE reviews;
ALTER TABLE reviews_new RENAME TO reviews;
CREATE INDEX reviews_newest ON reviews (game, created_at, id);

-- The position cache is keyed by the engine too. What Reckless's commits found cannot be asked for again.
DROP TABLE positions;
CREATE TABLE positions (
    key      TEXT NOT NULL,
    engine   TEXT NOT NULL,
    nodes    INTEGER NOT NULL,
    multipv  INTEGER NOT NULL,
    result   TEXT NOT NULL,
    PRIMARY KEY (key, engine, nodes, multipv)
) STRICT, WITHOUT ROWID;

DROP TABLE old_labels;

-- What only engine development used.
DROP TABLE train_steps;
DROP TABLE nets;
DROP TABLE datasets;
DROP TABLE tune_steps;
DROP TABLE trials;
DROP TABLE ideas;
DROP TABLE builds;
DROP TABLE versions;

DELETE FROM work WHERE kind NOT IN ('review', 'import', 'calibrate');
UPDATE work SET priority = 2 WHERE priority = 1;
UPDATE work SET priority = 4 WHERE priority = 3;
DELETE FROM connections WHERE key = 'bot';
DELETE FROM meta WHERE key LIKE 'upstream.%' OR key LIKE 'harness.%' OR key LIKE 'regression.%'
    OR key LIKE 'lichess.bot.%';
