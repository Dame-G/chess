-- Drill themes (core::themes) and hints. `kind` is NULL for a drill from before themes until the app classifies it from
-- its game's review; `detail` is the cause the evidence names. An answer given after a hint is `hinted`, which counts
-- half in a hit rate.
ALTER TABLE drills ADD COLUMN kind TEXT;
ALTER TABLE drills ADD COLUMN detail TEXT;
CREATE INDEX drills_kind ON drills (kind);
ALTER TABLE drill_answers ADD COLUMN hinted INTEGER NOT NULL DEFAULT 0;
