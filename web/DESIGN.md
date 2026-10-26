# Dameless: design

Dameless is a workbench for one chess engine: look at positions with it, change its code, and let thousands of games decide whether a change helps. It should feel like **VLC crossed with Lichess**: one window that plays things, a library you pick from and an inspector that explains what you see. It is not a website of pages.

## The shell (`App.vue`)

- **Header:** logo, five places (Board, Lab, Engines, Tune, Lichess) and Search (`Ctrl K`, an `ECommandPalette` listing places, actions, experiments and engines).
- **Status bar** (the `EAppFooter`): what the runner is doing, live, in mono. Clicking it opens the runner drawer. This is the only place runner status appears. Don't repeat it on a page.
- **Workspace routes** (`meta.workspace`) lock to the window (`100dvh`). Nothing outside the panes scrolls. The home page is a cover and scrolls normally.

## The workspace (`components/Workspace.vue`)

Every place has the same three panes, flat and divided by hairlines:

| Pane                                                     | Role                                                                                                                                    | Width                   |
| -------------------------------------------------------- | --------------------------------------------------------------------------------------------------------------------------------------- | ----------------------- |
| Library (left, `library`, `library-head`)                | What you pick from: experiments, games, builds, positions. Dense rows, keyboard-navigable, the selected row marked with `aria-current`. | `--ws-library`, 18rem   |
| Stage (centre, default slot, `stage-head`, `stage-foot`) | What you are looking at: the board, an experiment, a build.                                                                             | the rest                |
| Inspector (right, `inspector`, `inspector-head`)         | What you read about it: engine lines, moves, stats.                                                                                     | `--ws-inspector`, 24rem |

Selection lives in the URL (`/lab/3`, `/engines/2`, `/games/7/12`), so every view can be linked, and Back works. Below 64rem the panes stack in this order: stage, inspector, library.

## Places

- **Home `/`** is a cover, like eos.dame.gg's home page: a full-window animated chequer field, the wordmark, one line of tagline, a filled button and one outline button. Nothing else: no metrics, no lists.
- **Board `/board`, `/games/:test/:index`** is the player:
  - **Stage:** eval bar and board.
  - **Transport** (`stage-foot`): first, back, play/pause (auto-advance), forward, last; a scrubber that is the eval graph; the move counter; speed.
  - **Inspector:** the engine (eval, best move, lines) above the move list, as on Lichess.
  - **Library:** for a game, its test's games as a playlist (next game when one ends). For free analysis, the start position, paste, and recent positions.
  - Query parameters: `fen`, `moves`, `build`, `paste=1`.
- **Lab `/lab`, `/lab/:id`**:
  - **Library:** experiments, with a filter and search; "Test an idea" is the one filled button.
  - **Stage:** the chosen experiment, with a verdict sentence, the funnel, the live test with gauge, Elo and the LLR chart.
  - **Inspector:** that test's games, opening `/games/...`.
  - `?new=1` opens the new-experiment dialog.
- **Tune `/tune`, `/tune/:id`** is where the engine tunes its own search constants (SPSA):
  - **Library:** tunes, the live one first, with its iteration progress; "New tune" is the one filled button (`?new=1` opens it).
  - **Stage:** a sentence on where the tune stands, a trajectory chart (every parameter as a share of its own range; point at a line to read its real value, pick one in the table to highlight it) and the parameter table. Actions in the bar: Stop (confirmed), Resume, Apply (a dialog: branch, experiment, LTC, push; the result links to the lab).
  - **Inspector:** the tune's settings and its recent iterations.
- **Engines `/engines`, `/engines/:id`**:
  - **Library:** builds, plus the build form.
  - **Stage:** the chosen build: fingerprint check, speed compared with the others, network, compile log, "Analyse with it".
  - `?build=1` focuses the build form.

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
