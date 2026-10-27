-- R3, the player side (docs/roadmap.md): drills and their answers, and Dameless's levels: who sat at each game played
-- at a level, and the levels' calibrated ratings.

-- One drill per position (key: the FEN without move counters). The schedule is SM-2's (core::drills::grade).
CREATE TABLE drills (
    id            INTEGER PRIMARY KEY,
    key           TEXT NOT NULL UNIQUE,
    game          INTEGER NOT NULL REFERENCES games (id) ON DELETE CASCADE,
    ply           INTEGER NOT NULL,
    fen           TEXT NOT NULL,
    side          TEXT NOT NULL,
    best_moves    TEXT NOT NULL,
    best_san      TEXT NOT NULL,
    line          TEXT NOT NULL,
    played        TEXT NOT NULL,
    played_san    TEXT NOT NULL,
    swing         REAL NOT NULL,
    opening       TEXT,
    phase         TEXT NOT NULL,
    due           TEXT NOT NULL,
    interval_days INTEGER NOT NULL,
    ease          REAL NOT NULL,
    reps          INTEGER NOT NULL,
    lapses        INTEGER NOT NULL,
    created_at    TEXT NOT NULL,
    answered_at   TEXT
) STRICT;
CREATE INDEX drills_due ON drills (due);

-- Every answer, for the hit-rate trend. `first` marks a drill's first answer: new drills started today count against
-- the day's cap.
CREATE TABLE drill_answers (
    id      INTEGER PRIMARY KEY,
    drill   INTEGER NOT NULL REFERENCES drills (id) ON DELETE CASCADE,
    at      TEXT NOT NULL,
    answer  TEXT NOT NULL,
    outcome TEXT NOT NULL,
    first   INTEGER NOT NULL
) STRICT;
CREATE INDEX drill_answers_at ON drill_answers (at);
CREATE INDEX drill_answers_drill ON drill_answers (drill, at);

-- A game played at a level: each seat as an entrant (`level:<id>` or `you`). Calibration games are the background
-- matches between levels; the others are yours against a level.
CREATE TABLE level_games (
    game        INTEGER PRIMARY KEY REFERENCES games (id) ON DELETE CASCADE,
    white       TEXT NOT NULL,
    black       TEXT NOT NULL,
    calibration INTEGER NOT NULL
) STRICT;

-- The last calibration fit (core::levels::fit): one row per entrant, You included.
CREATE TABLE level_ratings (
    entrant    TEXT PRIMARY KEY,
    elo        REAL NOT NULL,
    low        REAL NOT NULL,
    high       REAL NOT NULL,
    games      INTEGER NOT NULL,
    updated_at TEXT NOT NULL
) STRICT;
