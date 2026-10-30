-- Maia is no longer a built-in: its nine engines leave the list. Games against them keep their seats as they were.
DELETE FROM engines WHERE id LIKE 'maia-%' AND kind = 'builtin';

-- Whether an engine declares UCI's MultiPV, read when it is added or installed (null: not known, read as yes). Without
-- it, the low levels play their node budget only.
ALTER TABLE engines ADD COLUMN multipv INTEGER;

-- Every engine has its own ladder of levels: a game at a level names the engine whose levels sat in it, read from the
-- seat that played at a level.
ALTER TABLE level_games ADD COLUMN engine TEXT NOT NULL DEFAULT 'stockfish';
UPDATE level_games SET engine = coalesce((
    SELECT CASE
        WHEN level_games.white LIKE 'level:%' AND g.white_key LIKE 'engine:%' THEN substr(g.white_key, 8)
        WHEN level_games.black LIKE 'level:%' AND g.black_key LIKE 'engine:%' THEN substr(g.black_key, 8)
    END
    FROM games g WHERE g.id = level_games.game
), 'stockfish');
CREATE INDEX level_games_engine ON level_games (engine);

-- The fits are per engine too. The one fit there was is Stockfish's.
CREATE TABLE level_ratings_new (
    engine     TEXT NOT NULL,
    entrant    TEXT NOT NULL,
    elo        REAL NOT NULL,
    low        REAL NOT NULL,
    high       REAL NOT NULL,
    games      INTEGER NOT NULL,
    updated_at TEXT NOT NULL,
    PRIMARY KEY (engine, entrant)
) STRICT;
INSERT INTO level_ratings_new (engine, entrant, elo, low, high, games, updated_at)
    SELECT 'stockfish', entrant, elo, low, high, games, updated_at FROM level_ratings;
DROP TABLE level_ratings;
ALTER TABLE level_ratings_new RENAME TO level_ratings;
