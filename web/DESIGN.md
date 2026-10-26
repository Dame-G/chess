# Dameless: design

Dameless is a workbench for one chess engine: look at positions with it, change its code, and let thousands of games decide whether a change helps. It should feel like **VLC crossed with Lichess**: one window that plays things, a library you pick from and an inspector that explains what you see. It is not a website of pages.

## The shell (`App.vue`)

- **Header:** logo, four places (Board, Improve, Versions, Lichess) and Search (`Ctrl K`, an `ECommandPalette` listing places, actions, experiments and versions).
- **Status bar** (the `EAppFooter`): what the runner is doing, live, in mono. Clicking it opens the runner drawer. This is the only place runner status appears. Don't repeat it on a page.
- **Workspace routes** (`meta.workspace`) lock to the window (`100dvh`). Nothing outside the panes scrolls. The home page is a cover and scrolls normally.

## The loop

The app exists to run one loop, and each place is one step of it:

1. **Make a branch.** A change to the engine, as a branch in the fork (Claude or the user writes it).
2. **Test it** (Improve). The branch plays against main through a funnel of stages until the evidence is clear.
3. **Read the verdict** (Versions, Improve). Each version shows passed, rejected or untested against main.
4. **Tune it** (Improve). A tune adjusts the search constants by SPSA and writes them to a new branch, which goes back to step 2.
5. **Use it.** Merge a passing branch into main in the Reckless repo, or put it on Lichess from Versions. Watch it or play it on the Board first.

## The workspace (`components/Workspace.vue`)

Improve and Versions have the same three panes, flat and divided by hairlines:

| Pane                                                     | Role                                                                                                                       | Width                   |
| -------------------------------------------------------- | -------------------------------------------------------------------------------------------------------------------------- | ----------------------- |
| Library (left, `library`, `library-head`)                | What you pick from: tests and tunes, versions. Dense rows, keyboard-navigable (arrow keys), the chosen row `aria-current`. | `--ws-library`, 18rem   |
| Stage (centre, default slot, `stage-head`, `stage-foot`) | What you are looking at: a test, a tune, a version.                                                                        | the rest                |
| Inspector (right, `inspector`, `inspector-head`)         | What you read about it: a test's games, a tune's iterations, build speeds.                                                 | `--ws-inspector`, 24rem |

Selection lives in the URL (`/improve/experiments/3`, `/versions/test%2Fno-nmp`), so every view can be linked, and Back works. Below 75rem the panes stack in this order: stage, inspector, library, and a "Library" button in the stage head scrolls to the list. The library head is `improve/LibraryHead.vue`; arrow-key stepping is `improve/libraryNav.ts`.

## Places

- **Home `/`** is a cover, like eos.dame.gg's home page: a full-window animated chequer field, the wordmark, one line of tagline, a filled button and one outline button. Nothing else.
- **Board `/board`, `/matches/:id`, `/games/:test/:index`** is the player: one board with an eval bar, a transport (first, back, play, forward, last; a scrubber that is the eval graph; speed) and a right panel with Moves and Games tabs. Modes: Analyse (live multipv), Play (you against a build, with clocks, takeback and draw offer) and Engines (two builds live). The New menu starts from the start position, a pasted PGN/FEN, the latest test games or a recent position.
  - **Review** (a finished match at `/matches/<id>`, `?ply=N` opens at a move): "Review this game" queues a review job and the panel follows it; once reviewed, the panel leads with each side's accuracy, ACPL and counts of `!!` `!` `?!` `?` `??` (a count jumps to the next such move), then the move on the board: its judgement and score swing, the engine's line, "Show best move" (the position before the move, with the better move drawn in teal) and "Play best line" (a free board from there). The move list carries the marks, the eval graph the review's scores with a dot per judged move, and `X` / `Shift X` walk your inaccuracies and worse. Nothing of it shows, and no review can be asked for, while a game of yours is on at Lichess.
- **Improve `/improve`, `/improve/experiments/:id`, `/improve/tunes/:id`** holds everything being tried, newest first with live ones on top:
  - **Library:** tests and tunes in one list, filtered by All / Tests / Tunes, a search and batches. "Test a change" is the one filled button; its dialog also starts a tune (`?new=test&dev=<branch>`, `?new=tune&branch=<branch>`). "How it works" explains the loop.
  - **Stage (a test):** a verdict sentence, the funnel as tabs (fingerprint, quick screen, short games, long games) and the evidence of the chosen stage: gauge, Elo, game pairs, the evidence chart. Stop is in the head.
  - **Stage (a tune):** where it stands in a sentence, the trajectory chart and parameter table. Stop (confirmed), Resume and Apply (a dialog: branch, test against main, long games, push) are in the head.
  - **Inspector:** the test's games (each opens `/games/...`) or the tune's recent iterations.
- **Versions `/versions`, `/versions/:name`** (a name with a slash is one encoded segment; an old build number redirects to its branch) is one row per branch:
  - **Library:** `main` first as the baseline, then the fork's branches newest first, each with its verdict against main, a tune badge and a Lichess mark. Hidden and upstream versions sit behind one toggle. The header menu frees disk space (a dry run first).
  - **Stage:** a verdict in plain words and the next step as the one filled button (Test it; once passed and built, Put it on Lichess), then the other actions that apply (Tune it, Watch it play, Play it, Build now; those that need a build appear only once built), then its builds with per-build Delete. The menu analyses with it, hides it or deletes the branch (typed confirmation).
  - **Inspector:** the speed of each version's newest build, and what the fingerprint means.
- **Lichess `/lichess`** is where the engine meets Lichess, as two identities chosen by a switch at the head of the stage: **You** (default) and **Your bot** (`?as=bot`), and a third choice, Compare (below). The library, on the left, lists the games of both, filtered All / Yours / Bot.
  - **You, logged out:** one filled button, "Log in with Lichess" (PKCE; the browser returns with `?lichess=connected` or `?lichess=error&message=`, which the page reads and cleans from the address). "Use a personal token instead" sits quietly below with the three scopes and a link to Lichess's token form with them ticked.
  - **You, logged in:** the account (title, ratings per speed, one sentence of state and activity), then Play: a switch between "Find a game" (a seek: rapid and slower only, a clock, colour, rated, an optional rating range; while looking it shows "Looking for an opponent…" and Cancel) and "Challenge" (a friend, Lichess's Stockfish at level 1 to 8, or My bot; blitz is allowed here). One line explains why a seek has no bullet or blitz. The inspector lists challenges (Accept, Decline, Cancel; a challenge the Board API cannot play says "Accept on Lichess") and imports your history ("Import my games", then "Fetch new games"). Log out asks first.
  - **Your bot:** connection (the Online switch), finding games, settings (build, challenges accepted), and the log of challenges. Nothing here acts on the other identity.
  - **Anywhere in the app** (`App.vue`): a game of yours that starts opens on the board when you are on Lichess, and otherwise offers it in a toast ("Your game against X has started", Open). While one is live the status bar says "Playing on Lichess · vs X" (a link to it) and, when the fair-play lock is on, "Analysis is off". An incoming challenge toasts with Accept unless you are on the Lichess page.
- **Compare `/lichess/compare`** is the third choice of the Lichess switch (You, Your bot, **Compare**; it has its own address, so Back works). It sets players side by side over their reviewed games, as a stat sheet: players are columns, rows are grouped. What is compared lives in the address (`?players=me,Magnus,bot&speeds=blitz&rated=1&limit=50`), so a comparison can be bookmarked; `me` is always first.
  - **Library (Players):** who is in it (You, Your bot if there is one, any number of Lichess players), a username field (checked against Lichess before it is added; a pasted profile link works), the games counted (speeds with Lichess's meanings, rated only, the newest 20, 50, 100 or 200 of each player) and "Top blitz players now" from the leaderboard (Bullet, Blitz or Rapid), one tap to add.
  - **Stage:** the sheet, with a sticky label column that scrolls sideways on a phone. Each column head says who, the rating, "leads N of M rows" and "20 of 50 reviewed", with a "Few games" tag under ten. The one thing to do for a column is in its head: "Import last N games" (a player with none here), then "Review N games". Rows: Accuracy (overall, opening, middlegame, endgame, per speed, centipawn loss), Moves per game (a stacked bar of the classes, then each class with its Lichess glyph, per game with the total in grey), Results (score with wins, draws, losses, as White, as Black, performance, opponents, longest win streak), Time per move and Most played openings. The best value of a row has a tint, the primary colour and a note for screen readers. Ties, and rows where a column has fewer than three reviewed games, have no leader; a column with a few games is drawn quieter.
  - **Inspector (Moments):** each player's best moves (brilliant, else great), worst blunders and most and least accurate game, each a link into the board at that ply (`/matches/<id>?ply=<n>`).
  - **Nothing reviewed:** one filled button, "Review my last N games", with the cost in words (about 20 s of CPU a game). While a review runs, a progress bar sits above the sheet. With the fair-play lock on, reviews and imports are disabled and one calm notice says why.
  - **Colours** of the move classes (`compare/compare.css`, `--q-*`) follow Lichess's glyph colours (inaccuracy blue, mistake amber, blunder red) with teal for brilliant and blue for great. They carry meaning, never decoration, and lift in dark mode.

## Words

One vocabulary across every screen, the command palette and the docs. Use these, not synonyms.

- **Baseline:** `main`. Never "reference", "base" or "current version".
- **Version:** a branch of the engine, shown by its branch name. Commit hashes appear only on hover.
- **Verdicts:** Passed, Rejected, Stopped, Running, Queued. Nothing else ("Stronger", "Not stronger", "Waiting", "Testing").
- **Actions:** "Test a change" and "Tune numbers" (not "New test", "New tune"). "Watch two versions play", "Improve the engine".
- **Test stages:** Quick screen (4+0.04), Short games (8+0.08), Long games (40+0.4).
- **Clocks:** written `3+2`, minutes plus seconds a move. Bullet, Blitz, Rapid and Classical mean Lichess's speeds only (initial + 40 × increment; the `lichessSpeed` glossary entry).
- **Review words:** Accuracy, Centipawn loss, and the classes Brilliant, Great, Best, Excellent, Good, Book, Inaccuracy, Mistake, Blunder and Miss (glossary entries, shown with Lichess's glyphs). A review is the engine searching every position of a game; Compare sets reviewed games side by side.
- **Lichess identities:** "You" is the person's own account, "Your bot" the BOT account. "Find a game" is a seek in the lobby; "Challenge" names an opponent. Never "player" or "human account".
- **Fair play:** "Analysis is off while you're playing on Lichess". The lock is stated once, in the status bar; the board explains it where analysis would be.
- **Links:** a control that goes to another page is a link (middle-click and copy-link work), never a button that navigates.

## Look

- **Dense and sharp:** theme radius 3, depth 1, compact density, snappy motion (`theme.ts`). Prefer hairline dividers and flat panes over floating cards with shadows. A card inside a pane is the exception.
- **Type:** Hanken Grotesk for reading, JetBrains Mono for every number, ref, commit and time (`.num` for tabular figures). Fraunces only for the wordmark and the home title.
- **Colour carries meaning:**
  - Brand cognac for the current item and the primary action.
  - Success, warning and danger only for verdicts.
  - White and black on the board's own wood.
- **No redundancy:** each fact appears once per screen. Prefer one sentence ("Leaning towards failing, about 20 minutes to go") over three widgets saying the same thing.
- **Plain words first, jargon on demand:** `Term.vue` explains Elo, SPRT, bench and so on in a toggletip.

## Rules (from eos's design principles)

- Compose from eos components, imported by path (`@eos-ui/vue/components/EButton`). **Never use a native `<button>`, `<input>` or `<select>`.** A pattern eos lacks belongs in eos. Note it rather than hand-rolling a control.
- The parent owns space (`gap`, never outer margins). Use semantic tokens: `--e-gap-*`, `--e-pad-*`.
- One filled button per screen, named with a verb.
- Sentence case. Lead with the status, then the reason, then the next step.
- Logical properties, container-friendly layouts, `prefers-reduced-motion` respected, keyboard first (`useShortcuts` from `@eos-ui/vue`).
