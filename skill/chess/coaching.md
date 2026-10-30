# Coaching the user

How to read the user's games with the app and help them improve: reviews, profiles, comparisons, drills, the repertoire and levels. Every tool here that analyses is refused with 409 `fair_play` while the user plays on Lichess; wait for the game to end.

## Getting the games

- The user's Lichess history: `import_player` with `key: "you"` (`chess import you --wait`): the first run brings in the whole history (newer games, then back until Lichess has no older one), later runs only newer games. Another player's: `key: "lichess:<name>"` (`chess import lichess:<name> --max 50`). Both are background work.
- Pasted PGN: `import_pgn` (`{ pgn, you?: "white" | "black" }`).
- `list_games` filters by `player` (`you`, `engine:<id>`, `lichess:<name>`), `source` (`local`, `lichess`, `import`), `speed`, `rated`, `since`; `get_game` and `get_game_pgn` read one.

## Reviews

A review searches every position of a finished game with the analysis engine (200,000 nodes a position, MultiPV 2 by default; `engine` names another) and judges each move as Lichess does. Each review names the engine that made it.

- Queue: `review_games` with `games: [ids]` or `player: "you"` (its newest unreviewed games, `limit` 20). Every unreviewed game: `player: "you", background: true` (`chess review --player you --all`), newest first at the lowest priority; it can take days, so do not wait on it. It is work: `wait_for_work`. A game already reviewed with the same engine and budget is skipped; "Review all" leaves alone every game any engine has reviewed.
- Read: `get_review` (`chess game <id> --review`): per side accuracy, ACPL, counts per classification and per phase, and every move with `eval_before`, `eval_after` (White's point of view), `best_san`, `best_pv`, `win_before`, `win_after` (the mover's win %), `classification` and `phase`.
- Judgements are Lichess's: a drop in the mover's win % of 5, 10 and 15 points is an inaccuracy, a mistake and a blunder. Classifications add `brilliant`, `great`, `best`, `excellent`, `good`, `book` and `miss` (a mistake right after the opponent's, letting a winning chance go).
- When explaining a mistake, give the position, the move played, the better move and its line, and the idea in plain words (what it wins or prevents), the way a coach would. One or two lessons per game land better than a list of every inaccuracy.

## Profiles and comparison

- `get_profile` (`chess profile you --speeds blitz,rapid`): results by colour, accuracy overall and per phase, openings, time use, the trend, best moments and worst blunders, and **weak spots**: positions before the user's mistakes, recurring themes first (same opening family and phase).
- `compare_players` (`players: "you,engine:stockfish,lichess:<name>"`): profiles side by side with who leads each row. Engine figures count reviewed games only; queue reviews first when `pending_review` is high.
- `get_progress`: the user's rating, accuracy and drill hit rate over time.

## Drills

Drills are the user's own mistakes as exercises: find the move they missed, on a spaced schedule (a miss returns tomorrow, a pass in growing intervals).

- `make_drills` turns weak spots of reviewed games into new drills; reviews of the user's games also make them.
- `drills_today` lists what is due without the answers. `answer_drill` (`{ move }`, UCI or SAN) grades the user's answer and returns the right moves and the line.
- When drilling the user, show the position and side to move from `drills_today`, take their move, and pass it to `answer_drill`. Reveal the answer only from `answer_drill`'s result, after they answered: `list_drills` carries the answers, so keep it for managing drills.

## Repertoire and levels

- `get_repertoire` (`side: "white" | "black"`): the lines the user plays, how they score in each, and where they leave theory (`departures`). Useful for "what should I study": the departures with poor scores.
- Levels are the analysis engine at a chosen strength (`list_levels`: about 800 to full, calibrated by the app). A local game at a level: `start_game` with a seat such as `stockfish@5`; a human-like opponent at a rating: `maia-1500`; from a weak spot, `from: { type: "position", fen, moves }`. Local games against an engine are practice, not Lichess, so fair play does not limit them.
