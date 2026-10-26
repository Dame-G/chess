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
- **Improve `/improve`, `/improve/experiments/:id`, `/improve/tunes/:id`** holds everything being tried, newest first with live ones on top:
  - **Library:** tests and tunes in one list, filtered by All / Tests / Tunes, a search and batches. "Test a change" is the one filled button; its dialog also starts a tune (`?new=test&dev=<branch>`, `?new=tune&branch=<branch>`). "How it works" explains the loop.
  - **Stage (a test):** a verdict sentence, the funnel as tabs (fingerprint, quick screen, short games, long games) and the evidence of the chosen stage: gauge, Elo, game pairs, the evidence chart. Stop is in the head.
  - **Stage (a tune):** where it stands in a sentence, the trajectory chart and parameter table. Stop (confirmed), Resume and Apply (a dialog: branch, test against main, long games, push) are in the head.
  - **Inspector:** the test's games (each opens `/games/...`) or the tune's recent iterations.
- **Versions `/versions`, `/versions/:name`** (a name with a slash is one encoded segment; an old build number redirects to its branch) is one row per branch:
  - **Library:** `main` first as the baseline, then the fork's branches newest first, each with its verdict against main, a tune badge and a Lichess mark. Hidden and upstream versions sit behind one toggle. The header menu frees disk space (a dry run first).
  - **Stage:** a verdict in plain words and the next step as the one filled button (Test it; once passed and built, Put it on Lichess), then the other actions that apply (Tune it, Watch it play, Play it, Build now; those that need a build appear only once built), then its builds with per-build Delete. The menu analyses with it, hides it or deletes the branch (typed confirmation).
  - **Inspector:** the speed of each version's newest build, and what the fingerprint means.
- **Lichess `/lichess`** is the bot account: connection, settings (build, challenges accepted), finding games, the log of challenges and the games played.

## Words

One vocabulary across every screen, the command palette and the docs. Use these, not synonyms.

- **Baseline:** `main`. Never "reference", "base" or "current version".
- **Version:** a branch of the engine, shown by its branch name. Commit hashes appear only on hover.
- **Verdicts:** Passed, Rejected, Stopped, Running, Queued. Nothing else ("Stronger", "Not stronger", "Waiting", "Testing").
- **Actions:** "Test a change" and "Tune numbers" (not "New test", "New tune"). "Watch two versions play", "Improve the engine".
- **Test stages:** Quick screen (4+0.04), Short games (8+0.08), Long games (40+0.4).
- **Clocks:** written `3+2`, minutes plus seconds a move. Bullet, Blitz and Rapid mean Lichess's speeds only.
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
