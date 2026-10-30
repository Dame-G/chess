-- A trial's SPRT error rates. Trials from before them ran at Reckless's alpha 0.05 and beta 0.10, which is how a null
-- reads back for a trial with bounds.
ALTER TABLE trials ADD COLUMN alpha REAL;
ALTER TABLE trials ADD COLUMN beta REAL;
