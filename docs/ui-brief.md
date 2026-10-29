# Dameless UI overhaul: design brief

Status: audit and direction, written by the design director. Implementers: read sections 1 to 4 first (everything depends on them), then your area in section 7 and the matching parts of sections 5 and 6.

Audit evidence is in `docs/ui-audit/` (PNGs, NOT yet git-ignored: add `/docs/ui-audit` to .gitignore before committing; regenerate with the scripts described at the end). Every screenshot named below lives there. The fixtures used: `?fixtures`, `?fixtures=empty|error|loading|newcomer|long|settings-edge`. `real-*.png` are read-only shots of the live app at :7979.

Vocabulary rules from AGENTS.md still hold (you, Dameless, version, champion, idea, trial, game, player, work, review, connection). Never in UI copy: build (except diagnostics), branch (except the idea form), match, experiment, test, batch, job, runner, human.

---

## 0. The verdict in one page

What works: the Saddle palette (cognac, brass, linen / espresso) is distinctive and warm; the board stage is genuinely good; the information is honest and well-worded; the app is already close to token-clean (almost no stray hex, gaps come from eos tokens).

What does not:

1. **Nothing has a rank.** Every section is the same grey uppercase mono kicker over the same content weight. Selected segmented buttons, primary buttons, current tabs, links, stamps and the live dot are all the same cognac. The eye has no first stop on any screen.
2. **Sheets are 2,000 px scrolls.** You, Dameless and Settings are single columns of 8 to 12 sections squeezed into a 510 px column beside a dead board. The user cannot find anything without scrolling a river.
3. **The same thing is built five ways.** Section headers (`.sec`/`.part-title`/ENotice titles/inline kickers), key figures (`.figs`, Metric, bare numbers, WdlBar rows), tables (EDataGrid/ETable/custom divs), stamps, empty states, segmented filters, dialogs: each screen invents its own.
4. **The board area changes size and structure between modes** (647 px play, 593 px review, 727 px drill at 1440x900) and wastes the screen at 1920 (770 px board in a 1120 px stage). At 768 the board is 768 px square and pushes everything off screen.
5. **The rail lies.** On the real data the Games rail shows one trial row and "No games here yet" under You, with 3,708 imported games, because the app fetches `limit=300` and trial games fill all 300. This is a data bug that reads as a design failure.
6. **Mobile is the desktop stacked.** No navigation, a 30-row move list between the board and everything else, the games rail at the very bottom of the page.

The direction ("Workbench v2"): keep Saddle and radius 3, make it **quieter and ranked**.

- One loud colour use per screen (the primary action). Selected state becomes ink-on-tint with an underline, not a solid cognac slab.
- Outcome colour (win green, loss red) is only for results and judgements.
- Everything numeric is mono, right-aligned, tabular; everything else is sans, sentence case. Uppercase mono kickers are removed from forms and tables and survive only as the one section label style.
- Sheets become **tabbed pages** with a real header (title, subtitle-as-data, actions) and a sticky tab row. Three screens-worth of scroll become three tabs of one screen each.
- Mobile gets a **bottom tab bar** (Board, Games, You, Dameless).
- A small `src/ui/` kit replaces every ad hoc pattern, and the rule is: **no screen styles its own section header, stat, table, empty state, badge or dialog frame.**

---

## 1. Principles

1. **One focus per screen.** Each view has exactly one primary thing (a board, a verdict, a form) and at most one filled button. Everything else is quiet. If two things are loud, one of them is wrong.
2. **Say it once.** A fact appears in one place per screen. A heading is not repeated by its first sentence; a stamp is not repeated by its prose; a count is not shown in both the tab and the table. Descriptions under titles are removed unless they teach something the title does not.
3. **Numbers are data, words are labels.** Figures: mono, tabular, right-aligned in columns, units in a smaller muted run. Labels: sans, sentence case, muted, 12 or 14 px. Never mono-uppercase for a label, never sans for a figure.
4. **Colour has roles, not moods.** Cognac = the one action and the current selection marker. Green/red/amber/teal/blue = outcomes and judgements only. Neutrals do the rest. Links are ink with an underline, not cognac (cognac links next to cognac buttons and cognac selections are indistinguishable).
5. **One way to do each thing.** One section header, one stat, one table, one key/value list, one empty state, one badge, one segmented control, one dialog frame, one confirm. Built once in `src/ui/`, used everywhere.
6. **Dense, not cramped.** Fine-pointer targets 32 px, coarse 44 px; rows 28 / 32 / 48 by type; breathing room comes from consistent gaps between groups (16, 24), not from tall rows.
7. **The board never moves.** The board is the same size and place in every mode of a given window; modes change what is around it, never it. Panels yield to the board, sheets yield to the panel.
8. **Every state is designed.** Loading is a skeleton of the final shape (no layout jump), empty teaches the next action in one sentence and one button, error says what failed and offers Try again, in the same frame as the content.
9. **Works at 375 first.** Every screen is specified at 375, 768, 1280, 1920. Desktop is what you get when there is room, not the baseline.

---

## 2. Tokens and scales

All new tokens go in `src/base.css` under `:root` (and the dark overrides under `:root[data-scheme="dark"]` where noted). Components use these; they never use a bare rem, px or hex, except the allowed exceptions listed at the end of this section. eos tokens are mapped, not replaced, so eos controls (buttons, fields, dialogs) still line up.

### 2.1 Spacing (4 px grid)

eos's compact density ships 3 / 6 / 9 / 12 / 18 px gaps, which is not a grid and makes our custom 0.125/0.25/0.375/0.1875rem literals drift. Move to a 4 px grid and alias the eos gaps onto it so eos components follow:

```css
:root {
  --sp-1: 0.25rem; /*  4  hairline gaps: icon to text, chip internals, stacked label/value */
  --sp-2: 0.5rem; /*  8  inside a control group, between inline items, row padding-block */
  --sp-3: 0.75rem; /* 12  panel/sheet inner padding, between a label and its field, cell padding-inline */
  --sp-4: 1rem; /* 16  between blocks inside a section, dialog padding, sheet gutter on desktop */
  --sp-5: 1.5rem; /* 24  between sections */
  --sp-6: 2rem; /* 32  page-level breathing (empty states, sheet top) */

  /* eos follows the grid */
  --e-gap-tight: var(--sp-1);
  --e-gap-snug: var(--sp-2);
  --e-gap-base: var(--sp-3);
  --e-gap-loose: var(--sp-4);
  --e-gap-section: var(--sp-5);
}
```

Foundation owner: after aliasing, screenshot every eos control (Button, Segmented, Field, Dialog, Tabs) at both schemes and fix any that regress; if eos reads `--e-space-*` instead (it does for some), alias those too (`--e-space-1: var(--sp-1)`, `-2: sp-2`, `-3: sp-3`, `-4: sp-4`, `-6: sp-5`, `-8: sp-6`).

Rules of use:

| Where                           | Value                                                                                             |
| ------------------------------- | ------------------------------------------------------------------------------------------------- |
| Sheet / panel inner padding     | `--sp-3` (mobile) `--sp-4` (>= 56rem)                                                             |
| Between sections in a sheet     | `--sp-5` with a hairline (see 2.5), no extra padding on both sides                                |
| Between blocks inside a section | `--sp-4`                                                                                          |
| Section header to its content   | `--sp-2`                                                                                          |
| Label to field in a form        | `--sp-1`                                                                                          |
| Between form fields             | `--sp-3`                                                                                          |
| Dialog padding                  | `--sp-4`; footer padding `--sp-3` `--sp-4`                                                        |
| Rail row padding                | `--sp-2` block, `--sp-3` inline                                                                   |
| Table cell                      | `--sp-1` block, `--sp-2` inline; first/last cell `--sp-3` if the table touches the container edge |

`--ws-pad` (0.75rem) in `App.vue` becomes `var(--sp-3)`; delete `--ws-pad` and use the token.

### 2.2 Type scale

Two families already loaded: Hanken Grotesk Variable (sans), JetBrains Mono Variable (figures), Fraunces only in the wordmark. Collapse the many sizes (0.75, 0.8125, 0.875, 1, 1.125, 1.25, 1.5 and several clamp()s) to this:

| Token             | Size / line     | Weight | Family | Use                                                                         |
| ----------------- | --------------- | ------ | ------ | --------------------------------------------------------------------------- |
| `--t-title`       | 1.25rem / 1.3   | 600    | sans   | Sheet title, dialog title, empty-state title                                |
| `--t-head`        | 1rem / 1.35     | 600    | sans   | Section title (the one section label style), card title                     |
| `--t-body`        | 0.875rem / 1.45 | 400    | sans   | Everything: panels, sheets, rows, forms, tables. Default for the whole app. |
| `--t-body-strong` | 0.875rem / 1.45 | 600    | sans   | Row primary text, table first column, field values                          |
| `--t-meta`        | 0.75rem / 1.35  | 400    | sans   | Muted secondary: labels on stats, axes, helper text, timestamps             |
| `--t-fig`         | 0.875rem / 1.3  | 500    | mono   | Figures in tables, lists, strips, rail                                      |
| `--t-fig-lg`      | 1.25rem / 1.2   | 600    | mono   | Stat values                                                                 |
| `--t-fig-xl`      | 1.75rem / 1.1   | 600    | mono   | The one hero figure of a screen (accuracy in review, rating on You)         |
| `--t-clock`       | 1.125rem / 1    | 600    | mono   | Board clocks                                                                |

Body text becomes 14 px app-wide (today the body is 16 px, panels fall back to 14 px, and sheets mix both). The `--e-type-body` the eos shell uses stays for eos internals; set `body { font: var(--t-body) }`.

Delete: `.part-title` (replaced by `UiSection` header), `0.8125rem` everywhere, uppercase letter-spaced mono kickers in forms and tables (`type-kicker` as a label). eos `EField` labels: override to `--t-meta` muted sentence case (`:where(.e-field-label)`), 500.

Section label (the one allowed uppercase): not used. Section titles are `--t-head` sentence case, ink, with a trailing hairline. This is what makes the hierarchy: titles are now actually bigger and darker than content, where today the kicker is smaller and lighter than content (so the eye reads content first and never sees structure).

Mono rule: only figures, move notation (SAN/UCI/FEN), clocks, versions/branch names. Names (players, openings, ideas) are sans.

### 2.3 Colour roles

Keep the Saddle theme inputs. Add semantic aliases so components never choose a hue:

```css
:root {
  /* surfaces */
  --c-page: var(--e-background); /* app background, behind the board */
  --c-pane: var(--e-surface); /* rail, panel, sheet, dialog */
  --c-raise: color-mix(in oklab, var(--e-ink) 4%, var(--e-surface)); /* table header, code, inset, hover */
  --c-sunk: color-mix(in oklab, var(--e-ink) 7%, var(--e-surface)); /* selected row, segmented track */
  /* ink */
  --c-ink: var(--e-ink);
  --c-ink-2: var(--e-muted); /* labels, meta; already meets 4.5:1 */
  --c-ink-3: color-mix(
    in oklab,
    var(--e-muted) 70%,
    var(--e-surface)
  ); /* disabled, axis ticks only; never for text that carries information */
  /* lines */
  --c-line: color-mix(in oklab, var(--e-border) 40%, transparent); /* hairline dividers (replaces .ws --line at 55%) */
  --c-line-strong: var(--e-border); /* control borders, table outer border */
  /* action */
  --c-act: var(--e-primary); /* primary button fill, the selected marker bar, the live dot, focus */
  --c-act-ink: var(--e-on-primary);
  --c-act-wash: var(--e-primary-soft); /* selected row/tab wash, 15% */
  /* outcomes (the only place green/red are allowed besides status dots and stamps of the same meaning) */
  --c-win: var(--e-success);
  --c-loss: var(--e-danger);
  --c-draw: var(--e-muted);
  --c-warn: var(--e-warning);
  --c-info: var(--e-info);
  /* judgements: already --rv-* in board/review.css; keep, and add --rv-best: var(--c-ink-2) */
}
```

Rules:

- **Filled cognac** = one primary button per view (and `Start`, `Save`, `Promote to vN` in dialogs). Nothing else is filled cognac. Today selected segments, active filters, the review bar's Review all, the New idea button, the Stop button hover and the theme chooser are all filled. Selected segments become: `background: var(--c-sunk)`, `color: var(--c-ink)`, `font-weight: 600`, plus a 2 px `--c-act` bar under the label. (Implement as `UiSeg` variant, section 3.)
- **Links** (in sheets, prose, rows that navigate): `color: inherit; text-decoration: underline; text-decoration-color: var(--c-line-strong); text-underline-offset: 0.2em`, hover underline becomes ink. Glossary terms (`Term.vue`): dotted underline in `--c-ink-3`, no colour. Current links are cognac with a solid cognac underline and collide with the primary action.
- **Status** (running, queued, kept, dropped, failed, connected): a `UiBadge` (3.9), not a boxed mono uppercase stamp in six colours. Tone = neutral | win | loss | warn | info | act.
- **Dark mode**: the same names; define overrides only where eos's derived value fails contrast (check `--c-ink-3` and `--c-line` in dark; line mix must be 30% not 40%).
- **Board colours** stay in `board.css` (they are not UI colours). The eval bar and graph use `--graph-white`/`--graph-black` which are hard-coded hex inside `EvalGraph.vue`/`EvalBar.vue`; move them to `--eval-white: #fffaf3; --eval-black: #2a2420` in `board.css` so the three share it. Exceptions allowed for hex: `board.css`, `review.css`, chessground shape colours in `Board.vue`.

### 2.4 Radii, borders, elevation

| Token      | Value                 | Use                                                          |
| ---------- | --------------------- | ------------------------------------------------------------ |
| `--r-0`    | 0                     | Table internals, board edges against the stage, strips       |
| `--r-1`    | 3px (`--e-radius-md`) | Controls, badges, inputs, rows' selected state, board, cards |
| `--r-pill` | `--e-radius-pill`     | The live dot, WDL/penta bars' ends only                      |

Only two radii in the app. The theme's `radius: 3` stays; remove the `--e-radius-sm` (1.5px) usage in custom CSS.

Borders: hairline `1px solid var(--c-line)` between regions and sections; `1px solid var(--c-line-strong)` around controls and tables only. **No box around the sheet title** (see issue S-1). **No box around a whole table** unless it scrolls horizontally: header row on `--c-raise` with a bottom line, rows separated by hairlines, no vertical lines.

Elevation: none inside panes. Dialogs/popovers use eos `--e-shadow-float`; the board keeps its current frame. Remove `depth`-based stripes from eos buttons: `--e-depth` stays 1px but buttons get no bottom slab (the "snappy slab" under every button reads as chunky next to a dense table). Foundation owner: try `depth: 0` in `theme.ts` and compare; keep only if the controls stay legible. This is the one theme change; if it looks flat, revert and note it.

### 2.5 Dividers

Pane-to-pane (rail | stage | panel): one `1px var(--c-line)`. Section-to-section in a sheet: the section's top hairline plus `--sp-5` above and `--sp-3` below the line (today `padding-block: base` on both sides of every border-top, which makes sections of different heights look unevenly spaced). Rows in lists: hairline between, none after the last.

### 2.6 Density targets (row heights)

| Thing                                      | Fine pointer | Coarse (`pointer: coarse`)                |
| ------------------------------------------ | ------------ | ----------------------------------------- |
| Button / input / select / segmented option | 32           | 44                                        |
| Icon button                                | 32           | 44                                        |
| Header bar                                 | 48           | 48                                        |
| Status footer                              | 32           | removed (moves into the header indicator) |
| Rail row (2 lines)                         | 48           | 56                                        |
| Move row                                   | 28           | 40                                        |
| List row, one line                         | 32           | 44                                        |
| List row, two lines                        | 48           | 56                                        |
| Table row                                  | 28           | 40                                        |
| Table header                               | 24           | 32                                        |
| Tab (panel and sheet)                      | 36           | 44                                        |

Set `--e-control-height: 2rem; --e-tap: 2rem` under `@media (pointer: fine)` in `base.css` (today 2.5rem/2.25rem in `App.vue`, which gives 40 px buttons that are the biggest thing in a panel full of 14 px text) and `2.75rem` under coarse. Tokens: `--h-control`, `--h-row`, `--h-row-2`, `--h-table-row`, `--h-move`, `--h-tab` for the rest, defined once.

### 2.7 Icons

Phosphor (already used). Sizes: 14 (inline with `--t-meta`), 16 (default, in buttons and rows), 20 (header, tab bar), 28 (empty states). One weight (regular); bold only for the check/close in a selected state. Delete `--e-icon-size-*` overrides in components. Icon-only buttons always have `aria-label` and a tooltip on fine pointers.

### 2.8 Motion

Keep eos snappy. Add: `@media (prefers-reduced-motion: reduce)` kills sheet slide, the live-dot pulse and the loading shimmer (already partly). New: the live dot pulses once per move (not continuously).

### 2.9 Layout tokens

```css
:root {
  --w-rail: clamp(16rem, 18vw, 20rem); /* 256..320 (today 288..352) */
  --w-panel: clamp(22rem, 24vw, 28rem); /* 352..448 */
  --w-sheet: clamp(32rem, 34vw, 44rem); /* sheets dock here beside the board */
  --w-sheet-wide: clamp(40rem, 52vw, 64rem); /* Compare, You > Openings, Dameless > Ideas tables */
  --w-measure: 62ch;
}
```

---

## 3. Homebrew components (`src/ui/`)

Build in `src/ui/`, one `.vue` per component plus `src/ui/index.ts` is NOT used (import by path, as the app does today). Prefix `Ui`. All are slotted, token-only, no scoped hard-coded sizes, with tests in `tests/ui/` for the logic bits (UiTable alignment, UiResult mapping, UiScore sign/colour).

### 3.1 `UiSection.vue`

The one section frame.

```
props: title: string; level?: 1 | 2 (default 1); collapsible?: boolean; open?: boolean (v-model); id?: string
slots: default; #aside (right side of the header: filters, a link, a button); #note (one muted line under the title)
```

Renders `<section>` with a header row: title (`--t-head`) left, `#aside` right (wraps below the title under 30rem), then `#note` in `--t-meta`, then content with `--sp-2` gap. Level 1 has the top hairline and `--sp-5` margin from the previous section; level 2 is the same without the hairline and `--t-body-strong`. **Replaces**: `.sec`, `.sec-head`, `.sec-note`, `.part-title` (base.css), and every inline `<h3 class="part-title">` in `PlayPanel`, `ExplorePanel`, `ReviewPanel`, `BotCard`, `BrainCard`, `LevelsCard`, `DrillsCard`, `ProfileView`, `RepertoireView`, `SettingsSheet`, `IdeaSheet`, `CompareSheet`, `Logbook`, `TrainView`, `TuneView`.

### 3.2 `UiStat.vue` and `UiStats.vue`

```
UiStats props: columns?: number (default auto-fit min 7rem); dense?: boolean
UiStat props: label: string; value: string | number | null; unit?: string; delta?: number | string; tone?: 'win'|'loss'|'warn'|'info'; hint?: string (tooltip + glossary key); size?: 'md'|'lg'|'xl'; sub?: string
slots: default (custom value), #label (to add a Term), #sub
```

Label above in `--t-meta` muted (a `Term` dotted underline only if `hint`), value below in `--t-fig-lg` (or xl for the screen's hero), unit in `--t-meta` after the value, `sub` below in `--t-meta`. `null` value renders an en dash in `--c-ink-3`, never "0" or "?" literal (real data shows `1345?` and `Accuracy: -`). **Replaces** `.figs/.fig/.figs--small` in base.css; the You header figures; Games/Score/Accuracy block; ReviewPanel's hero numbers; TrialEvidence's Elo/nElo/Pairs; Strength numbers; `EMetric` uses in sheets.

### 3.3 `UiTable.vue` (DataTable)

For every table in the app: levels, openings, networks, datasets, moves-per-game, compare, top players, repertoire, trial games, harness.

```
props: columns: { key: string; label: string; align?: 'start'|'end'|'center'; width?: string; fig?: boolean (mono tabular); sort?: boolean; hide?: 'sm' | 'md' (hide below breakpoint) }[]
       rows: T[]; rowKey: keyof T | (r) => string; dense?: boolean (default true); selected?: string; sticky?: boolean (header sticks inside its scroll parent)
       empty?: string; loading?: boolean (skeleton rows)
slots: #cell-<key>="{ row, value }"; #head-<key>; #foot; #row-aside (expand)
emits: select(row), sort(key, dir)
```

28 px rows, hairline separators, header 24 px `--c-raise` sentence-case `--t-meta` 500 (**not** mono uppercase), numeric columns right-aligned `--t-fig`, first column `--t-body-strong`, zero borders at the sides, horizontal scroll inside with a fade edge on overflow (fixes the Openings table whose header text is clipped at 510 px). Selected row `--c-sunk` + 2 px act bar at the start edge. Hover `--c-raise`. **Replaces**: EDataGrid/ETable instances and hand-rolled grids in `LevelsCard`, `BrainCard`, `ProfileView` (openings, moves per game), `CompareSheet`, `TopPlayers`, `RepertoireView`, `Logbook` rows, `TrialEvidence` games list.

### 3.4 `UiKeyValue.vue`

```
props: items?: { label: string; value: string|number; mono?: boolean; copy?: boolean; tone? }[]; columns?: 1|2; dense?: boolean
slots: #value-<key> | default (<UiKeyValue><UiKv label="Cores">15</UiKv></UiKeyValue>)
```

Label left (`--t-meta`, 9rem fixed on desktop, stacked above on mobile), value right-flowing `--t-body`, mono only if the value is a path/commit/number. Row height `--h-row`, hairline between. **Replaces** Settings' ACCOUNT / LOGIN ENDS / CORES / ENGINE CHECKOUT / DATA / BINARIES / SERVER / VERSION list (EDescriptionList with mono uppercase labels), the idea sheet's `Clock 8+0.08 · played at …` line, the Compare header facts, and PlayPanel's "5+3 · local game" line (as a `compact` inline variant: `<UiKeyValue inline>`).

### 3.5 `UiEmpty.vue`

```
props: icon?: Component; title: string; body?: string; tone?: 'neutral'|'error'; dense?: boolean (inline in a section: one line, no icon)
slots: #actions (max one primary + one quiet)
```

Centered in its container with `--sp-6` top padding, 28 px icon in `--c-ink-3`, title `--t-head`, body `--t-body` muted max 36ch, actions row. `dense` = one muted line with an inline link button (replaces `.sec-note`). `tone="error"` has a danger icon, no red panel (today the rail error is a full-height pink box). **Replaces** every EEmptyState, the Notice-as-empty in rail, `.sec-note` empties ("None waiting. Challenges to you on Lichess show here.", "No drills yet…", "No moves yet…").

### 3.6 `UiToolbar.vue`

```
props: align?: 'start'|'between'; wrap?: boolean (default true); sticky?: boolean
slots: default; #end
```

A flex row, gap `--sp-2`, min-height `--h-control`, hairline bottom when `sticky`. For: rail filter row, panel mode row, sheet filter row (speed/rated), Transport, Review bar, Explore engine row, game actions (Offer a draw / Take back / Resign). Children `UiSeg`, buttons, `UiSelect`.

### 3.7 `UiPlayer.vue` (PlayerChip)

The one way a player is named anywhere.

```
props: seat: Seat | PlayerRef; rating?: number | string | null; provisional?: boolean; colour?: 'white'|'black'; size?: 'sm'|'md'; link?: boolean (to /players/:key); live?: boolean; title?: string
```

Renders: colour pip (square 10 px white/black outlined; the seat's side, replacing the orange live dot that currently means both "live" and "side"), name (`--t-body-strong`; "You" is the user's account name only in profile contexts), `BOT`/`FM`/`NM` as a `UiBadge` (neutral, tiny), rating in `--t-fig` muted with `?` shown as `1345?` only when `provisional`. **Replaces**: `Strip.vue` player half, `GameRow` names, `PlayerSheet`/`CompareSheet` headers, header `EStatLink`s (via size `md`), Top players rows, Idea games rows.

### 3.8 `UiScore.vue` (ScoreBadge)

```
props: score: Score (cp | mate) | number (pawns); perspective?: 'white'|'mover'; size?: 'sm'|'md'
```

`+0.6` / `-4.2` / `#3`: mono, sign always shown, one decimal, mate as `#n`. Colour: **no colour** by default (white advantage is not "good"); a small square pip of the side ahead sits left of it (white or black fill). Tone only when `perspective="mover"` (profile contexts). **Replaces**: EvalBar figure, strip eval chip (`+0.6` in the 768/375 strip is a white box that clashes with the clock), Explore engine lines, graph tooltip, Review's move line, Trial LLR labels where they show a signed value. One function `formatScore` in `format.ts` is the source.

### 3.9 `UiBadge.vue` (Stamp)

```
props: tone?: 'neutral'|'win'|'loss'|'warn'|'info'|'act'; solid?: boolean; dot?: boolean
```

18 px high, `--t-meta` 600 sentence case ("Running", "Kept", "Near your rating", "Connected"), 1.5 px inner padding, radius `--r-1`, tinted fill (12% tone) with tone-coloured text, **no border, no mono, no uppercase**. `solid` only for LIVE. **Replaces**: every `EStamp` (RUNNING, QUEUED, KEPT, DROPPED, STOPPED, FAILED, PASSED, CONNECTED, READY, CHECK, TRUSTED, NEAR YOUR RATING, BOT, FM). Mapping table lives in `src/ui/badge.ts` (`ideaTone`, `trialTone`, `healthTone`).

### 3.10 `UiResult.vue` (ResultMark)

```
props: result: '1-0'|'0-1'|'1/2-1/2'|'*'|null; you?: 'white'|'black'|null; format?: 'score'|'letter'|'dot'
```

From the user's perspective when `you` is set: win = green `1-0`/`W`, loss = red, draw = muted `½-½`; without `you` (other people's games, trial games) always muted ink. Fixes the rail where `1-0` is green for every game regardless of who the user was (cutemouse83 - You `1-0` is a **loss** and shows red-ink `1-0` by accident of colour, "You - knightowl 0-1" shows red: inconsistent). Fixed-width (3 characters), mono `--t-fig`, right-aligned.

### 3.11 `UiSeg.vue` (segmented control, quiet)

Wrap `ESegmented` rather than reimplement; style via `:deep`.

```
props: modelValue; options: { value; label; icon?; count? }[]; size?: 'sm'|'md'; label: string (a11y); full?: boolean
```

No borders between options; track `--c-sunk`, selected option `--c-pane` with ink text 600 and 2 px act underline. `count` as `--t-fig` muted. Single source for: rail filter, speed, rated/casual, As White/As Black, Gain/Neutral/Speed/Tune/Net/Custom trial, time control, opponent, colour, theme (System/Light/Dark), bot "Looking for games", logbook Ended/Kept/Dropped.

### 3.12 `UiField.vue`

```
props: label: string; hint?: string; error?: string; optional?: boolean; for?: string
slots: default (the control)
```

Label (`--t-meta` 500, sentence case, "Optional" appended in muted instead of right-aligned), control, hint (`--t-meta`) or error (`--t-meta` danger). 4 px label gap, 12 px between fields. **Replaces** the ad hoc EField + uppercase kicker labels in NewGame, NewIdea, BotSettings, Datagen, ImportDataset, AddPlayer.

### 3.13 `UiDialog.vue`

```
props: open (v-model); title: string; size?: 'sm'(26rem)|'md'(32rem)|'lg'(40rem); busy?: boolean; danger?: boolean
slots: default; #footer-start (hints, validation like "Give it a title."); #actions (override);
emits: confirm; cancel
props for actions: confirmLabel: string; cancelLabel?: string (default Cancel); confirmDisabled?: boolean
```

Frame: title `--t-title` (single line, ellipsis; **no 2-line serif-ish wrap** like the Promote title), body `--sp-4`, sticky footer on `--c-raise` with **Cancel left, primary right** (today primary left, Cancel right, and Cancel is a white box that outweighs the filled button), Enter confirms, Escape cancels, focus goes to the first field (not the close button; fix: the New game dialog opens with a focus ring on the X). Close button 32 px quiet icon. Below 40rem the dialog is a bottom sheet (full width, 85dvh, handle bar). `danger` makes the confirm button red. **Replaces** eos `EDialog` usage in NewGame, NewIdea, BotSettings, Datagen, ImportDataset, and `ConfirmDialog` (Promote, Push, Resign, delete) through a thin `UiConfirm` composable `useConfirm()` returning `Promise<boolean>`.

### 3.14 `UiSheetPage.vue`

The frame every sheet uses (replaces direct `ESheet` + `useSheet` binding in 6 sheets).

```
props: title: string; subtitle?: string (data, e.g. "dame_g · Lichess"); tabs?: { id; label; count? }[]; tab?: string (v-model, synced to route hash/query ?tab=); wide?: boolean; loading?: boolean; error?: string|null
slots: #actions (header right: external link, primary button); #lead (the hero strip under the header, always visible above tabs); default (tab content, scope: tab id); #tab-<id>
```

Behaviour: header 48 px (back chevron if applicable, title, actions, close X) is **sticky**; hero strip (`#lead`, up to 2 rows of `UiStat`) scrolls away; tab row sticky below header, 36 px, `UiSeg`-style underline tabs; content scrolls. Title is `<h1>` with `tabindex="-1"` and `outline: none` (focus moves there for screen readers but no box is drawn: see issue S-1). Sheet width `--w-sheet`, `wide` -> `--w-sheet-wide`. Below 75rem it is full-window as today, with a back arrow instead of X. Loading shows the hero + tab skeleton at final geometry. Error shows `UiEmpty tone="error"` with Try again.

### 3.15 `UiPanelHead.vue` (the game card at the top of the board panel)

```
props: game: GameSummary | null; mode: Mode
slots: #status (what is happening now), #actions
```

Two lines: `UiPlayer white  [UiResult]  UiPlayer black`, then `Opening · 5+3 · rated · Lichess` in `--t-meta`. This is the one place the panel states who is playing whom; the board strips keep clock + name but the panel no longer repeats "Dameless played h6. Your move." plus "5+3 · local game". Replaces the first lines of Play/Watch/Review panels.

### 3.16 `UiLive.vue`

8 px dot, `--c-act` (live) or `--c-win` (connected) or `--c-loss` (error), optional pulse. Replaces dots in the rail, status line, header work indicator, Watch.

### 3.17 Utilities in `base.css`

`.num`, `.mono` stay. Delete `.sec*`, `.figs*`, `.fig*`, `.part-title`, `.plain`, `.sheet-actions` after migrating. Add `.stack` (grid gap `--sp-4`), `.row` (flex gap `--sp-2` wrap), `.muted`, `.truncate`, `.sr-only` if eos lacks it.

---

## 4. Layout

### 4.1 Shell

Three regions plus header, exactly as now, with these changes.

```
>= 75rem (1200)
+--------------------------------------------------------------------------------------+
| Dameless   [ Search or run a command  Ctrl K ]     [+ New game]  You 1,840  Dameless v14  [work] [gear] |  48
+----------+------------------------------------------------------------+--------------+
| Games    |                      stage (board)                         | panel / sheet|
| rail     |                                                            |              |
| 256-320  |                                                            | 352-448 /    |
|          |                                                            | sheet 512-704|
+----------+------------------------------------------------------------+--------------+
```

- **Status footer removed.** Its content ("Trying deeper-lmr · short games · 412 pairs · +0.7 ± 5.9 Elo · ~40 min (and 1 more)") becomes the **work indicator in the header** (`UiLive` + short label `Trial 412 pairs`, click opens a popover listing work with stop/pause). The footer eats 37 px of vertical space on every screen (board height!) for one line that nobody reads at the bottom. At 1440x900 this adds 37 px to the board. Mobile already does this (the stacked header has the indicator). **One indicator everywhere.** Files: `shell/StatusLine.vue` (becomes the popover content + compact trigger), `App.vue` (drop footer), `TopBar.vue`.
- **Header**: `Dameless` wordmark (the Fraunces italic `less` stays, it is the brand), centered command bar 26rem, right: `New game` (the one filled button, shown always, not only when you have no game running: playing a second game vs a bot while one runs is legal and the user should find the entry; when a game you sit in is live it becomes quiet), then the **two players as `UiPlayer size="md" link`**: `You 1,840` and `Dameless v14` with a vertical hairline between them and a faint `vs` glyph is NOT used; they are simply symmetric and visually paired (same chip shape, same height, current one has the `--c-sunk` background not the cognac outline), then the work indicator, then Settings. Currently `You 1,840` is plain text and `Dameless v14` is plain text; when on a sheet the active one becomes a cognac outlined box (a different shape from the idle state). Fix: both always chips.
- **Rail** 256 to 320 px (`--w-rail`), see 5.2.
- **Stage**: see 4.2.
- **Panel** `--w-panel`; sheets `--w-sheet` (or wide), see 4.4.
- Hairlines `--c-line` between regions; backgrounds: rail and panel `--c-pane`, stage `--c-page`.

Breakpoints (single list, defined once in `src/media.ts`, referenced by name not number): `sm` 30rem (480), `md` 48rem (768), `lg` 75rem (1200), `xl` 100rem (1600). Today the code uses 30, 40, 56, 75 rem in different files (header collapses at 56, new-game label at 40, stage at 40 and 75). Collapse to the list above.

| Width                | Layout                                                                                                                                                                                                                                                                                                                                                                                                                                                         |
| -------------------- | -------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| < 48rem (phone)      | Single column. Board edge to edge, then the panel content, then nothing else on the page. Navigation = **bottom tab bar** (4.5). The Games rail is its own tab (full screen). Sheets are full-screen pages with the tab bar still visible.                                                                                                                                                                                                                     |
| 48 to 75rem (tablet) | Two columns: board (sized by height) left, panel right (`--w-panel`), rail behind a left drawer button in the header (`Games` icon) that slides over. Never a 768 px square board (the audit's 768 shot scrolls 2,466 px). Sheets: right drawer `--w-sheet` over the panel, board stays.                                                                                                                                                                       |
| 75rem to 100rem      | Rail + stage + panel/sheet as above.                                                                                                                                                                                                                                                                                                                                                                                                                           |
| >= 100rem (1920)     | Same, but the panel grows to 28rem and the rail to 20rem; remaining width goes to the stage margins. The board is capped by height (see 4.2), so extra width is **not** given to the board: instead, below the board at >= 100rem and >= 56rem height, nothing is added. Accept margin; do not stretch. (The 1920 shot has a 770 px board with 330 px of margin each side; acceptable and calm. The fix there is smaller rail/panel gaps, not a bigger board.) |

### 4.2 The board stage

**Principle: the board never changes size or position when the mode, game or panel changes.** Reserve the chrome:

```
stage column (width = board + eval col)
  strip top      36   player + clock           (always rendered; empty-game shows a muted "Start position" strip? no: empty strip keeps height)
  board          S x S
  strip bottom   36
  graph slot     56   (always reserved; renders the graph in review/watch, an empty hairline baseline otherwise)
  transport      32
```

`S = min(100cqh - (36+36+56+32) - gaps - 2*pad, 100cqw - evalcol - 2*pad)`. Today `--strips` and `--graph` collapse to 0 in explore/drill, so the board grows by up to 130 px and jumps (measured: 593 px review vs 647 px explore vs 727 px drill at 1440x900). With the footer removed (+37 px) and always-reserved chrome, the board is about 650 px at 1440x900 in every mode. In explore/drill the strips show the side to move (`White to move` with the pip) and the position name/opening; the graph slot is empty. In drill the transport is replaced by Skip/Hint buttons in the same slot.

- **Eval bar**: 16 px wide (today 28 px column with a text label inside), full board height, figure shown as a `UiScore` chip **under the bottom strip's clock row on the right**, not rotated inside the bar. Not shown on phones (today absent at 375 in play but a white `+0.6` chip appears in the strip; use the same chip rule everywhere: score chip in strip when no bar).
- **Graph**: 56 px, neutral: line + area in `--c-ink-3`, black/white halves replaced by a single zero line with white-ahead area above and black-ahead below in 14% ink (today a black block fills the top half and white the bottom, which in dark mode and in light mode both read as a harsh slab; see `board-review-1440-light.png`). Markers for mistakes/blunders as dots in `--rv-*`. Hover readout as now.
- **Transport**: `|< < play > >| 33/33` and the flip button. Keep; make the counter `--t-fig` muted; icon buttons 32 px; align the whole group to the board's left edge (today it is offset by the eval column in one mode and centered in another).
- **Coordinates**: inside the squares, small. Fine; keep. Set the coordinate colour to follow the square (dark coordinate on light square) as now.
- **Phones**: board edge to edge, no eval bar, strips 32, graph 40. The board is square at 100vw.

### 4.3 The panel (right column on desktop, below the board on phones)

Structure, top to bottom, the same in every mode:

1. `UiPanelHead` (game card, 3.15) - 2 lines, hidden in free Explore.
2. Mode tabs **only when there is more than one mode** (today Play shows a single "Play" tab: remove; Explore alone: remove; Review/Explore and Watch/Explore: keep, 36 px underline tabs). When a mode has one tab, its name goes in the section title instead.
3. Mode content (<= 40% of the panel height; it never pushes the move list off-screen).
4. Move list, fills the rest, scrolls inside.

At 1280x800 the Review content currently overflows and clips "Play this out" (`board-review-1280-dark.png`): P1. The review summary becomes a compact block (5.4) that is 120 px tall, not 330.

### 4.4 Sheets

A sheet is a **page that docks beside the board**, not a scroll of everything.

- Dock width `--w-sheet` (512 to 704); `wide` variants `--w-sheet-wide` (>= 640) for You/Compare/Dameless tables. The stage shrinks (board re-fits) as today; the rail stays (it is how you open games from within a sheet).
- Every sheet has: sticky header (title, optional actions, close), optional hero strip, **tabs**, content. No sentence-long description under the title (remove "Your games added up: how you play, what to work on, and how you compare." etc.; the title and tabs say it). Subtitle is data only (account name, rating, version).
- Tabs per sheet (5.8 to 5.13): You = Overview, Games, Openings, Drills, Challenges(only when there are any); Dameless = Strength, Ideas, Brain, Bot; Settings = Accounts, Machine, Health, Appearance; Idea = Trials, Notes, Games; Player/Compare: none (single screen).
- Opening animation: slide from the right 120 ms, `prefers-reduced-motion` off. Closing returns to the board address as today.
- Under 75rem a sheet is full-screen with a back arrow; the bottom tab bar (4.5) stays.

### 4.5 Mobile navigation (new)

Bottom tab bar at < 48rem, 56 px + safe area: **Board**, **Games**, **You**, **Dameless**. Icons 20 + label `--t-meta`. Current tab has the act underline. Settings stays in the header (gear) with the work indicator. `Games` is a full-screen list (the rail component, `UiToolbar` filters sticky at the top). Choosing a game switches to Board. `New game` is a header + button on Board and a fab-less top action on Games. The page no longer ends in a 28 rem rail; the move list on the Board tab collapses to a one-row **horizontal move strip** under the transport (`1. e4 e5 2. Nf3 ...`, current move scrolled to center) with a "Moves" button opening the full list in a bottom sheet. Files: new `shell/TabBar.vue`, `App.vue`, `Panel.vue`, `MoveList.vue`, `GamesRail.vue`, `media.ts`.

### 4.6 Always visible

Header (48), the board stage (always the board, except mobile Games/You/Dameless tabs), the rail on >= 75rem, and the work indicator. Never hidden behind a sheet: the live game's clock (when a sheet is open and you sit in a live game, the header shows `UiPlayer` with the running clock of your game, `3:31`, as a chip next to New game; click returns to the board). Today a sheet over your live game hides your clock because the board shrinks but the panel (with the game status) is replaced.

---

## 5. Per-screen redesign

Convention: **Remove / Merge / Order / Copy / Ref**. Screenshots are in `docs/ui-audit/`.

### 5.1 Header (TopBar)

Ref: `main-1440-light.png`, `board-play-375-light.png`, `board-review-768-dark.png`.

- Remove: the status footer (moves into header, 4.1); the `You` and `Dameless` cognac outline "active" box; icon-less `You`/`Dameless` links with figures that disappear at 375.
- Merge: `StatLink`s into `UiPlayer` chips; the compact work indicator and the footer StatusLine into one component.
- Order: wordmark, command bar, New game, You chip, Dameless chip, work indicator, gear.
- Copy: command bar placeholder `Search or run a command` stays (fine). Tooltip on chips: `Your profile` / `Dameless: ideas, strength, the bot`. At < 30rem chips are icons (person, engine) with the rating/version dropped (today it shows the words "You" and "Dameless" without figures, which are meaningless labels).
- Mobile (< 48rem): wordmark glyph only, command icon, work indicator, gear. New game moves to the Board tab header; You/Dameless to the tab bar.

### 5.2 Games rail

Ref: `main-1440-light.png`, `real-main-1440.png`, `real-rail-you-1440.png`, `empty-1440-light.png`, `error-1440-light.png`, `board-play-375-light.png` (bottom), `board-review-768-dark.png`.

**Data (P1, a bug that looks like design):** `stores/games.ts` loads `limit: 300` newest and filters client-side. On the real app trial games fill all 300, so imported history never appears, You filter shows "No games here yet" with 3,708 games. Fix: the rail asks the server for the active filter (`player=you`, `source=…`, `trial` for Trials, none for All but with `exclude_trial` or by folding trial rows server-side) and pages 50 at a time with infinite scroll in the virtual list (a "Load more" row at the end). Keep the live games always loaded (they come over SSE). `All` = games you or Dameless played + live; trial games only under Trials (they are not "games" in the user's sense; AGENTS defines game as every record, but the rail's job is the user's attention: a 4,000-pair trial must be one folded row and never be in All beyond its single row; as now but not at the cost of the 300).

- Remove: the "live" mono word at row end (replace by the pulsing dot only; the word duplicates the dot); the accuracy pair `97.0 · 96.0` in mono bold on the second line for reviewed games (it competes with the names; move it to a trailing muted `97 / 96` with a tooltip, only when reviewed); the caret-only trial rows' extra indentation gutter on non-trial rows (rows have a fixed 16 px gutter that is empty for 90% of rows).
- Merge: the "You - Dameless" title format. Replace the hyphen with `-` en-dash rendering done through `UiPlayer` pair: `You  vs  Dameless` is wrong for AGENTS' "match" ban? No: `vs` is fine (the trial rows already say `deeper-lmr vs v14`). Use `vs` consistently (today games use `-` and trials use `vs`).
- Row (48 px): line 1: `UiPlayer(white) vs UiPlayer(black)` with names `--t-body-strong` ellipsis; trailing `UiResult` or live dot. Line 2 `--t-meta`: `10+5 · Rapid · rated` left, relative time right (`20h`). Selected row: `--c-sunk` + 2 px act bar at start (today full cognac outline box).
- Filter row: `UiSeg` sm: `All · You · Dameless · Lichess · Trials`, no wrap at 256 px (labels `All You Dameless Lichess Trials`: 5 options at 256 px = 51 px each; at `--w-rail` 256 use `Dameless` truncated? Instead: `All · You · Dameless · Lichess · Trials` at `--t-meta` 12 px fits 256 minus gutters; verify at 16rem).
- Add a search field? No: the command palette already finds games. Instead the filter row gets a trailing icon button `Import` only when You has 0 games (newcomer).
- Empty / error: `UiEmpty` (3.5). Copy: All empty: title `No games yet`, body `Play Dameless, or link your Lichess account to bring in your games.`, actions `New game` (primary) `Open Settings` (quiet). Per filter: You: `You haven't played here yet`; Dameless: `Dameless hasn't played yet`; Lichess: `No Lichess games` + `Import my Lichess games`; Trials: `No trials yet` + `New idea`. Error: `Games did not load` / `The app did not answer.` / `Try again`, no pink full-height box.
- Mobile: the rail is the Games tab (full screen), rows 56 px.

### 5.3 Play mode (you vs Dameless / on Lichess)

Ref: `board-play-1440-light.png`, `board-play-375-light.png`, `dlg-resign-1440-light.png`.

- Remove: the lone "Play" tab; the sentence "Dameless played h6. Your move." (the board shows the last move highlighted; show the **turn** only: `Your move` / `Dameless is thinking` as a `UiLive` line in the panel head); "5+3 · local game" (`UiPanelHead` meta line carries it as `5+3 · here`... use `5+3 · local`).
- Merge: Offer a draw, Take back, Resign into a `UiToolbar` right under the head: `Offer draw · Take back` quiet, `Resign` quiet-danger (red text, no fill); Resign confirms (UiConfirm, danger).
- Order: head, status line, actions, move list (starts directly under; no hairline gap box).
- Copy: `Offer a draw` -> `Offer draw`; `Take back` -> `Ask for takeback` only when against a person on Lichess, otherwise `Take back`; Resign dialog title `Resign this game?`, body `You lose the game.`, buttons `Cancel` (left) `Resign` (right, danger). Never leave it as a title that restates the button.
- Premove/promotion are unchanged.
- Mobile: actions are a sticky row above the tab bar while it is your move? No: one row under the transport.

### 5.4 Review mode (a finished game)

Ref: `board-review-1440-light.png`, `board-review-1280-dark.png`, `board-review-768-dark.png`, `board-review-lichess-1440-light.png`.

Current state is the most visibly broken panel: result line, a Rematch button, a two-column table with six judgement rows of mostly zeros, the best-move line, a Play-this-out button, then the moves. 330 px before the first move; at 1280x800 the Play-this-out button is clipped.

- Remove: `Brilliant 0 0` etc. **zero rows** (only show classes with a non-zero count for either side); the heading row `You / Dameless` repeated above the table (the head has the players); the word `Average centipawn loss` row as a full table row (becomes a stat); `Missed chance` row if zero.
- Merge: `Rematch` and `Play this out` into one `Play from here` split: the primary button (quiet) is `Play from this position` (the board's current position), and `Rematch` is an overflow item (`More: Rematch, Copy PGN, Copy FEN`). Copy FEN/PGN (today only in Explore) belong to a `UiToolbar` overflow in every mode.
- Order: `UiPanelHead` (players, result `1-0`, termination `Checkmate`, opening `Philidor Defense`), then a **review summary** (one compact block):

```
 Accuracy        95.0   94.3          <- UiStat xl, two columns, player names above (You, Dameless) in the head not here
 Avg. loss       22     17            <- UiStat md
 Moves           Blunder 1 · Mistake 0 · Inaccuracy 0   <- chips, only non-zero, each with its glyph (?? ? ?!) and rv colour; clicking jumps to that move
```

then **the current move card** (`17. Rd8#  Best move` with the glyph, the eval, and `Better: Qb3` with an arrow toggle) and the moves list. Judgement chips replace the table: click a chip to step through those moves (`n`/`p` keys already exist for this? if not, add: it is the most useful review interaction).

- Copy: `Best move` stays; `Missed chance` -> `Missed win` only for the lila `miss` class (keep the classification id, change the label), tooltip `You missed a chance to win material or the game.` Keep `Great move` -> `Great`. `Brilliant` stays. Word `Inaccuracy` fine.
- States: `Not reviewed` shows `UiEmpty dense`: `This game hasn't been reviewed.` + `Review it` primary (queues work), plus `about 1 min` estimate; running review shows a progress `n of 33 positions` bar with Pause; fair-play block shows the existing notice reworded: `Analysis is off while you play on Lichess.` (matches the server's 409 copy).
- Ref bug P1: `Play this out` clipped at 1280x800.

### 5.5 Watch mode (live game you are not in)

Ref: `board-watch-1440-light.png`.

- Remove: `The engine's line shows here while it thinks.` (when there is nothing to show, show nothing); the Watch tab if Explore is the only other (keep 2 tabs).
- Merge: description line into the panel head.
- Order: head (white vs black, `LIVE` `UiLive`, `3+2 · Blitz · rated`), engine thinking line (when present: `UiScore` + 1 line of PV, mono, ellipsis), moves.
- Copy: `Watching Alpha-Bot vs Dameless`... not needed (head). For bot games the head shows `Bot` badge.

### 5.6 Explore mode (free analysis / any position)

Ref: `board-explore-1440-light.png`, `board-explore-game-1440-light.png`, `real-main-1440.png`.

This is also what a first-time user lands on in the real app, so it matters.

- Remove: the `Space turns it on and off.` sentence (put the key in the tooltip and in Shortcuts); the `FEN` and `PGN` truncated text rows with their own `Copy` buttons (the user never reads a truncated FEN; copy is a single action); the `OPEN A POSITION` kicker; `No moves yet. Move a piece, or paste a FEN or PGN.` in the moves area (it is a section-level empty and appears below a hairline in a 200 px blank; show it in the move list slot as `UiEmpty dense`).
- Merge: FEN/PGN copy -> one `Copy` menu button (`Copy FEN`, `Copy PGN`) in the toolbar; paste field -> a one-line `Paste a FEN or PGN` input with an inline `Open` button (Enter opens). `Play from here` is the section's primary and sits in the toolbar next to Engine, not under the textarea.
- Order: toolbar `[Engine switch] depth 19 ........ [Play from here] [Copy v]`, engine lines (3 rows: `UiScore`, PV with ellipsis, click a PV to play it onto the board), `Paste a FEN or PGN` input, then the tree / move list.
- Engine lines: row 28 px, score right-aligned fixed width, PV `--t-fig` ellipsis with a `title`; the `+0.47 2. h3 a5 3. Bd3 …` starting at ply 2 (it does not show the first move number consistently; always render from the side to move: `1. e4 e5` or `1... c5`).
- Copy: `Engine` -> `Engine` (switch label) fine; `depth 19` -> `Depth 19`; `Play from here` stays; `Paste a FEN or a PGN` -> `Paste a FEN or PGN`.
- Fairness state (Lichess game running): replace the whole panel with `UiEmpty`: `Analysis is off while you play on Lichess.` + `Back to my game`.

### 5.7 Drill mode

Ref: `board-drill-1440-light.png`, `drill-375-light.png`.

- Remove: `One try: make the move on the board.` after the first view (show once per session via `stored`), `1 of 5 · Italian Game · opening` as a sentence.
- Merge: `Skip` and answer actions into one toolbar; progress `1 / 5` into a thin 3 px progress bar at the top of the panel.
- Order: the prompt `White to move: find the move you missed` (`--t-head`), meta line `Italian Game · opening` (muted), progress, toolbar `[Hint] [Skip]`. After answering: result card (`Correct` green / `Not this one` red) with the best line in a `UiScore` + SAN, a `Next` primary and `Explore this position` quiet. Today the panel is almost empty (200 px of text) beside a full-height board; make the answer state a designed card.
- Board strips show the side to move and the opening.

### 5.8 You sheet

Ref: `sheet-you-1440-light.png`, `sheet-you-tall-1440-light.png`, `sheet-you-375-light.png`, `newcomer-you-1440-light.png`, `you-error-1440-light.png`, `real-you-1440.png`.

Today: ~2,300 px single column: description sentence, name + On Lichess link, four ratings, challenges, two segmented filters (above the stats they filter, with no label), the review bar (a big tinted notice with a filled `Review all 23` button), an outlined `Import my Lichess games` button, 6 stats, a chart, accuracy by phase **and speed mixed in one list**, results, moves per game (two columns), openings table, progress chart, drills, repertoire. The filters scroll away from the content they filter.

New (page with tabs):

- **Header**: `You`; subtitle `dame_g` linking `On Lichess` (icon link in `#actions`). Remove the description sentence.
- **Lead (hero strip, always visible)**: ratings as `UiStat` xl `Rapid 1,846` with the others md (Bullet, Blitz, Classical) in one row; `?` provisional shown as `~` prefix and `provisional` tooltip, not a trailing question mark.
- **Tabs**: `Overview · Openings · Drills · Challenges 2` (count badge only when > 0; the tab is omitted when the connection is not linked).
- **Overview** (default): filters in a sticky `UiToolbar` at the top of the tab, labelled by a single line: speed `UiSeg` + `Rated | Casual | All` `UiSeg` (renamed `Rated and casual` -> `All`; `Rated` and `Casual` remain), applied to everything in all tabs; stats grid (`Games 64`, `Score 57.0%`, `Accuracy 79.4%`, `Avg. loss 35.0`, `Performance 1863`, `Win streak 6`, with `41 reviewed` as the `sub` of Games and Accuracy); the review/import prompt as a single `UiEmpty dense`-style banner: `23 games aren't reviewed yet (about 8 min in the background).  [Review all 23]  [Import more]` (primary button only if unreviewed > 0; otherwise `Import my Lichess games` quiet); the rating/accuracy-over-time chart (`LineChart`); then `Results` (`UiTable`-like WDL rows: All / As White / As Black with `WdlBar` and score); `Accuracy` as two small tables side by side on desktop: By phase (Opening, Middlegame, Endgame) and By speed (Bullet … Classical) with mini bars; `Moves per game` as one `UiTable` (class, per game, total) in two columns only on `wide`.
- **Openings**: `UiTable` with the opening, games, W/D/L bar, score; and `Your openings` repertoire tree (RepertoireView) below with White/Black `UiSeg`. Fix: header cells currently clip (`GAMES WON · DRAWN · LOST SCORE` run together); use real column labels `Games`, `Record`, `Score`.
- **Drills**: today's drills count + `Practice` primary + trend; empty: `No drills yet. Review your games and your mistakes become drills.` with `Review my games`.
- **Challenges**: the incoming/sent list with Accept/Decline/Cancel (move out of the top of the main sheet where it pushes everything down; the header shows a badge on the tab).
- **Newcomer** (0 games): replace the tabs with `UiEmpty`: `No games yet`, `Play one against Dameless, or bring in your Lichess history.`, actions `New game` primary, `Import my Lichess games`, `Log in with Lichess` when not linked.
- **Error/loading**: skeleton hero + 6 stat placeholders; error `UiEmpty tone=error` `Your profile didn't load` / `Try again`.
- **Windowing (the other agent's `ProfileFilters`/`players.ts`)**: the profile covers the newest 200 games (see `real-you-1440.png`: `200 games`, `Last 200 games`, `0 reviewed`). The `UiToolbar` must have a third control `Newest [All ▾ 50 / 200 / 1000 / All]` once that lands; until then show `Last 200 games` as muted text in the toolbar, never as an unexplained chart subtitle. Do not hard-code its location: `ProfileFilters.vue` just renders `UiToolbar` + `UiSeg` + `UiSelect`.

### 5.9 Dameless sheet

Ref: `sheet-dameless-1440-light.png`, `sheet-dameless-tall-1440-light.png`, `dameless-empty-1440-light.png`, `dameless-long-1440-light.png`, `real-dameless-1440.png`.

~3,000 px: Strength chart, Progress chart (a "You vs Dameless" comparison that belongs to the profile), 13-row Levels table, Ideas in progress, Logbook, Brain (networks, datasets, generate/import), Out in the world (bot).

- **Header**: `Dameless`, subtitle `Champion v14` (link opens the version). Remove the description.
- **Lead**: `UiStat xl` `Champion v14`, `+51.0 Elo over v1 (±12.8)`, `Playing: Alpha-Bot (blitz) ●` (live bot game as a chip linking the board), `Bot: online`.
- **Tabs**: `Strength · Ideas · Brain · Bot`.
  - **Strength**: the strength chart (champion line with the Elo band) + `Levels` as a **compact list**, not a 13-row table: a vertical ladder with a bar per level (742 to 3000) and a marker `You 1,846` between 6 and 7; each row `Level 6  1,822 ± 58  40 games`. `Calibrate levels` quiet button with its sentence trimmed to `About 1 min in the background.` Remove the Progress chart from here (it is already on You; link `Compare with you` instead).
  - **Ideas**: `New idea` primary in the tab header; **In progress** list (running first: badge, title, one line of evidence `+0.7 ± 5.9 Elo after 412 pairs`, an inline thin `LlrGauge`), **Logbook** below as one `UiTable` (Idea, Kind, Result, Elo, When) with the Ended/Kept/Dropped `UiSeg`; rows expand to the evidence as now (ExpandableRow is fine here).
  - **Brain**: Networks and Datasets as `UiTable`s with row actions in a kebab; `Generate data` and `Import a dataset` as quiet buttons in the section header (`#aside`), disk used as one `Data used 1.0 of 10 GB` meter row. Remove the hatched progress style (use a plain bar).
  - **Bot**: `Play on Lichess` switch + state line, `Looking for games` `UiSeg` (Off / Find one game / Keep playing; rename `Keep playing` -> `Always`), daily limit as `37 of 100 games against bots today` with a bar, the rules summary as a `UiKeyValue` (Plays: champion; Speeds: Bullet, Blitz, Rapid; Takes: rated and casual, people and bots; Clock: 1 to 30 min + up to 30 s; At once: 1) with `Edit rules` button replacing the paragraph "Rated and casual Bullet, Blitz and Rapid games, from 1 minute to 30 minutes with up to 30 s added a move, from people and bots, one at a time. Plays the champion." Recent bot challenges as a list below.
- States: `dameless-empty` (no ideas): Ideas tab `UiEmpty`: `No ideas yet` / `An idea is a change to test against the champion.` / `New idea`. `?fixtures=long` (many ideas): the logbook table virtualizes at 50 rows.
- Real app note (`real-dameless-1440.png`): version `v3`, few ideas; the Strength lead must degrade (`v3` has no Elo over v1 until there are two versions).

### 5.10 Idea sheet

Ref: `sheet-idea1-tall-1440-light.png`, `sheet-idea2-1440-light.png`, `dlg-promote-1440-light.png`, `sheet-idea3-1440-light.png`.

- **Header**: idea title (`Deeper LMR at high depth`), subtitle `Gain · by Claude · 2 hours ago`, status `UiBadge` (Running/Kept/...). Remove the hypothesis-as-description paragraph from above the status: it moves into the `Notes` tab as the first block (labelled `Hypothesis`), because it is the longest text and pushes the verdict down.
- **Lead**: the verdict sentence as a `UiStat xl`-style statement: `+0.7 ± 5.9 Elo` with `412 pairs, still running` or `Kept: +0.8 ± 2.1 Elo at short games, no worse.`; actions in the header: `Stop` / `Resume` quiet, `Promote` the primary only when kept. `search-oct` tag as a muted meta chip, not an outlined box button-look (today it looks like a button).
- **Tabs**: `Trials · Notes · Games`.
  - **Trials**: the stage strip (Fingerprint, Quick screen, Short games) as a stepper of equal 3-4 cells; the current stage expanded below it: LLR gauge (full width), the 3 stats (`Elo`, `nElo`, `Pairs`), Won/Drawn/Lost + pentanomial bar with labels `0 ½ 1 1½ 2`, the clock line (`Clock 8+0.08, played at 11.04+0.11 (x1.38)`). Fix: the two stat blocks `Elo / nElo / Pairs` and `Won · drawn · lost / Goal in nElo` currently align inconsistently (column 2 and 3 baselines differ) and the penta bar's numbers `80 640 1622 650 84` have no labels besides `0 ½ 1 1½ 2` in tiny type.
  - **Games**: `UiTable` (White, Black, Result, Moves) rather than underlined links `v13 - qsearch-evasions ½-½ 48 moves`.
- **Promote dialog** (`UiConfirm`): title `Promote to v15`, body `qsearch-evasions is merged into the fork's main on this machine and becomes the champion. Future ideas are tested against v15. Nothing is pushed.` Remove `If the checkout has uncommitted changes, nothing is merged and you are told why.` (it appears as an error if it happens).
- Error: unknown idea -> `UiEmpty`: `That idea isn't here`.

### 5.11 Player sheet (someone else)

Ref: `sheet-player-cm-1440-light.png`, `player-importing-375`.

- Header: `cutemouse83`, subtitle `Lichess · Blitz 1871 · Rapid 1862`; `On Lichess` as an icon link action. Remove `A player on Lichess, from the games brought in here.`.
- When not imported: the import state is the whole sheet: `UiEmpty`: `No games from cutemouse83 yet` / `Import their recent Lichess games to see how they play.` / `Import 50 games` primary. While importing: progress, `Stop`. Today the sheet is a 4-row sheet with a grey progress box and a lonely `Compare with you` button.
- When imported: same profile as You (Overview tab only, no drills/challenges) with `Compare with you` as the header primary and `Import more` quiet.
- Fair play: imports of others are refused while playing on Lichess: disable with tooltip `Off while you play on Lichess`.

### 5.12 Compare sheet

Ref: `sheet-compare-1440-light.png`, `sheet-compare-375-dark.png`.

- `wide` sheet. Remove the description and the `Speeds`/`Games`/`Newest games` kickers (use `UiToolbar` with `UiSeg` + `UiSelect` labelled in-control: `Newest 200 ▾`).
- Header cells: player chips with remove X; `41 of 64 reviewed` as `sub`.
- Table: a `UiTable` with sticky first column and sticky header; row groups as full-width subheaders (Results, Accuracy, Moves per game, Time per move, Openings); leader highlight is a **bold + dot**, not a mint cell fill (today the green cell makes half the table loud and in dark mode it is low contrast); ties both bold. `Longest win streak` wraps to two lines: widen the label column to 12rem on wide, or abbreviate `Best streak`.
- Remove `Rows that need the engine count reviewed games only. Open a player to review the rest.` as a trailing paragraph: it becomes a footer note `Rows marked ◦ count reviewed games only` with the ◦ glyph next to those row labels.
- Add player: `AddPlayer` stays as a toolbar-sized input at the top (`Add a player`) instead of the page bottom (today you scroll 1,600 px to find it).

### 5.13 Settings sheet

Ref: `sheet-settings-1440-light.png`, `sheet-settings-tall-1440-light.png`, `settings-edge-1440-light.png`, `sheet-settings-1280-dark.png`.

~2,100 px of key/value rows with six unlike things in one scroll.

- Header `Settings`. Remove the description.
- **Tabs**: `Accounts · This machine · Health · Appearance`.
  - **Accounts**: two cards side by side (`>= 56rem`): `You` and `The bot` each with `UiLive`, account name, `Connected` badge, `Login ends 25 Aug 2027`, `Log out` / `Disconnect` (quiet, with confirm), and for the bot a link `Bot controls` to the Dameless > Bot tab. Not-linked state: `UiEmpty dense` `Not connected` + `Log in with Lichess` primary / `Add the bot's token`.
  - **This machine**: one `UiKeyValue` (Cores, Engine checkout, Data, Binaries `23 binaries, 2.3 GB`), `Free disk space` as an inline action on the Binaries row (the explanatory sentence moves to its tooltip), the harness as `UiKeyValue` + `Check the harness` row action.
  - **Health**: the 9 diagnostics as a compact list: dot + name + one-line status (`fastchess 1.4.0`), the failing ones expanded with the fix (`Install it: winget install LLVM.LLVM` with a Copy button). Order: failures first. Remove the per-row `READY` stamps for healthy items: a green dot is enough; only non-ready items get a `UiBadge`.
  - **Appearance**: `Theme` `UiSeg` (System / Light / Dark) and the hosted app token section (`The hosted app`: server URL, version, `Copy token`).
- Copy: `THIS MACHINE` etc. no uppercase. `ENGINE CHECKOUT` -> `Engine checkout`. `Log out` stays; `Disconnect` -> `Disconnect the bot`.
- `settings-edge` state (long paths, missing tools): paths ellipsize in the middle with the full value in a tooltip and a copy button; never wrap a path.

### 5.14 Dialogs

All use `UiDialog`; frame sizes: New game `md`, New idea `md`, Bot rules `lg`, Datagen/Import `sm`, Confirms `sm`.

**New game** (ref `dlg-newgame-1440-light.png`, `dlg-newgame-375-light.png`)

- Remove: the uppercase kickers `OPPONENT`, `VERSION`, `STRENGTH`, `TIME CONTROL`, `YOU PLAY`; the helper `Minutes + seconds added each move.` (move to the placeholder/tooltip on the custom clock input); the helper under book line.
- Order: Opponent `UiSeg` (Dameless / Lichess / Stockfish); for Dameless one row `Version [v14 champion ▾]  Strength [Full ▾]` (if there is only one version, hide the Version select and show `v14`); Time control as a `UiSeg` of 5 presets + `Custom…` (a reveal of two number fields); You play `UiSeg` `White · Random · Black` (rename `Either` -> `Random`); advanced (`Start from a book line`, hash, threads) in a collapsed `Options` disclosure (the book switch is today a prominent control but rarely used).
- Footer: `[Cancel] ........ [Start game]`; the primary label includes the setup `Start 5+3 vs Dameless`, so the user can confirm what they are about to start without re-reading. Press Enter to start.
- Disabled opponents (Lichess unlinked, no Stockfish): visible with `UiBadge neutral` `Log in first` and the reason in the tooltip.
- At 375: bottom sheet, `UiSeg full`.

**New idea** (ref `dlg-newidea-1440-light.png`)

- Remove: the `A change to test against the champion.` sentence under the title; the helper `Commit the change on a branch with its Bench: line first.` as always-visible (show only as the error when a branch fails the check, as today `check-branch`).
- Order: Kind `UiSeg` + a one-line explanation that updates with the kind (`Should play stronger. Bench must change, then quick screen, then short games.`), Title, `What it should do, and why` -> `Hypothesis`, Branch (combobox) with live validation (`Bench line OK` / `Missing Bench line`), `Also play long games` switch with its hint folded into the tooltip, Tags (optional).
- Footer: `[Cancel] ... [Queue idea]`, the validation message `Give it a title.` moves to the field. Disabled-primary state uses the standard disabled look (today a dashed outline: remove the dashed border).
- Copy: `Custom trial` is jargon: `Single trial`.

**Bot rules** (ref `dlg-botrules-1440-light.png`)

- Order: Version; Takes challenges: `Rated`, `Casual`, `From people`, `From bots` as four checkboxes in a 2x2 (switches here feel like settings that apply now; checkboxes fit a form with a Save); Speeds checkboxes; Game length: **one range row** `Clock from [1 min] to [30 min]`, `Added time from [0 s] to [30 s]`, `Games at once [1]` with number fields not full-width steppers (the stepper rows today use ~50 px height each, six of them); `Clocks to look for games at` as a tag-style input (`3+0 3+2 5+3`); `Engine` disclosure for hash/threads/move overhead.
- Copy: `The bot's rules` -> `Bot rules`; remove the description; `Required` hint -> inline error.

**Generate data / Import dataset** (ref `dlg-datagen-1440-light.png`)

- Positions field `20 million`: a number field with the unit as suffix text `million`, not a stepper pair; Threads field with the default shown `7 (half of 15)`; Name optional with the hint inside the placeholder (`Reuse a name to add to it`); the estimate line `86.0 MB on disk, about 4 h to 5 h in the background.` becomes a summary row above the footer (`~86 MB · 4 to 5 h`). Import: the size confirmation is part of the dialog (`Download 7.9 MB from <host>?` with the host shown) and the primary label is `Download 7.9 MB`.

**Confirms** (Promote, Push, Resign, Delete dataset/build, Disconnect): `UiConfirm` only. Title is the action as a question, one sentence of consequence, `Cancel` left, action right, danger tone for destructive. Never a 2-line title.

**Command bar** (ref `dlg-command-1440-light.png`): sections `Places`, `Do`, `Games` as `--t-meta` headers; remove the per-row description line for obvious items (`Board`, `Settings`); show the shortcut chip (`g s`) as `Kbd` only; the result rows 32 px; the placeholder `Go to, find a game, or type: play 5+3 dameless` is good, keep. The panel sits over the board's top (fine); add `Esc` hint in the footer.

**Shortcuts**: a two-column grouped reference (Board, Review, Sheets, Anywhere); keep content, restyle with `UiKeyValue` and `Kbd`.

### 5.15 Global states

- **Loading** (`loading-1440-light.png`): skeletons in final geometry: rail rows (48 px x 6), panel head + 3 stat placeholders, sheet hero + tabs. No spinners except inside buttons. Delay skeletons 150 ms to avoid flash.
- **Error**: the page keeps working; a failed region shows `UiEmpty tone=error` with `Try again`. A global connectivity loss shows one thin banner under the header (`Can't reach the app. Reconnecting…`), not an error per region.
- **Toasts**: eos toasts fine; copy audit: sentence case, no trailing period if < 5 words.

---

## 6. Issue list

Severity: **P1** must fix, **P2** should, **P3** nice. Files relative to `apps/web/src`. IDs: G global, S sheets, R rail, B board/panel, D dialogs, M mobile, T tokens/base.

### G / T: Foundation, tokens, base.css

- T-1 **P1** Body is 16 px but panels/sheets use 14 px; sizes 0.75/0.8125/0.875/1/1.125/1.25/1.5 mixed. Introduce the 9-step scale (2.2), set `body { font: var(--t-body) }`. Files: `base.css`, 36 literals in `panel/*`, `parts/*`, `sheets/*`.
- T-2 **P1** Spacing uses eos 3/6/9/12/18 gaps plus literal 0.125/0.1875/0.25/0.375/0.5/1.25/5rem values. Adopt the 4 px grid and aliases (2.1); remove literals (`CopyRow`, `Notes`, `ProfileView`, `ReviewPanel`, `EvalBar`, `Strip`, `GameRow`, `Logbook`).
- T-3 **P1** Section headers: `.part-title` is smaller and lighter than content (12 px mono uppercase muted vs 14 px ink) so structure is invisible. Replace with `UiSection` (3.1). `base.css`, 15 files.
- T-4 **P1** Cognac is used for primary buttons, selected segments, selected tabs, links, current rail row outline, live dot, `On Lichess`, filters, stamps, glossary underlines. Role assignment (2.3). `base.css`, `theme.ts`, all.
- T-5 **P2** `--line` is defined inside `App.vue`'s `.ws` (55% mix) and used by children via inheritance; `sheets/*` use `var(--line)` outside `.ws`? (they are children of `.ws`; OK) but the name collides conceptually with `--e-stroke`. Move to `:root --c-line`.
- T-6 **P2** Control height: `App.vue` sets 2.5rem fine-pointer, header 2.25rem, rail 2rem, list rows 2rem: three different button heights in one view. Unify (2.6).
- T-7 **P2** Selected segmented and active filter = solid cognac slab: heaviest element in every sheet. `UiSeg` quiet style (3.11).
- T-8 **P2** Boxed uppercase mono `EStamp`s in six colours (RUNNING, KEPT, PASSED, CONNECTED, READY, CHECK, TRUSTED, NEAR YOUR RATING, BOT, FM): 25 instances. `UiBadge` (3.9); drop READY for healthy.
- T-9 **P2** Table headers are uppercase mono 11 px with letter-spacing and run together (`GAMES WON · DRAWN · LOST SCORE`, `NETWORK TRAINED ON Loss`). `UiTable` (3.3).
- T-10 **P2** Links are cognac with solid underline (`On Lichess`, `v14`, idea titles, glossary terms) and look like buttons or labels; glossary terms (`Accuracy`, `Centipawn loss`, `Performance`, `Elo`, `nElo`, `Pairs`) are cognac-underlined and look clickable. Link style + dotted muted terms (2.3). Files: `parts/Term.vue`, `base.css`.
- T-11 **P3** `--e-depth` slab under every button; consider depth 0. `theme.ts`.
- T-12 **P3** Hex colours in `EvalBar.vue` (#2a2420, #fffaf3) and `EvalGraph.vue` (`--graph-*` duplicates): single `--eval-*` tokens in `board/board.css`.
- T-13 **P3** `z-index: 10` in `board/board.css:193` and `CompareSheet.vue:256` raise to eos tokens (`--e-z-raised`).
- T-14 **P3** Three different dark-mode line colours (rail border, table border, section hairline). One `--c-line`.

### B: Board, stage, panel, move list

- B-1 **P1** Board size changes between modes (647 / 593 / 727 px at 1440x900). Reserve strips/graph/foot always (4.2). `board/BoardStage.vue`.
- B-2 **P1** Review panel overflows at 1280x800; `Play this out` is clipped under the move list (`panel/ReviewPanel.vue`, `panel/Panel.vue` `.tabs { max-height: 55dvh }` with `overflow: hidden auto` and the list grid `minmax(12rem, 1fr)`). Compact review block (5.4).
- B-3 **P1** At 768 the board is 768 px square with the panel below; page is 2,466 px tall. Tablet two-column layout (4.1). `App.vue` breakpoints.
- B-4 **P1** Single-tab tablists (`Play`, `Explore`, `Drill`) show an orphan tab. Hide when one mode; `panel/Panel.vue`.
- B-5 **P2** The panel head states things twice: `Dameless played h6. Your move.` + `5+3 · local game`; `1-0 Checkmate · Philidor Defense` + the strips' result `0`/`1`. `UiPanelHead` (3.15).
- B-6 **P2** Eval graph: black block fills the top 50% and white the bottom (harsh, reads as UI chrome); in review the strongest element below the board is the least useful. Neutral graph (4.2). `board/EvalGraph.vue`.
- B-7 **P2** Eval bar: 28 px wide with the figure printed inside on a contrasting patch, cropped `+0.6` at the bottom, mismatched with the strip chip at < 75rem. 16 px bar + `UiScore` chip (4.2). `board/EvalBar.vue`, `board/Strip.vue`.
- B-8 **P2** Strip: player name, `1840` rating, the score box and the clock are four unrelated styles (`Dameless  0  9:45` with a lone `0` captured-material or score? it is a points figure with no label). Replace with `UiPlayer` + material diff `+3` (muted) + clock. `board/Strip.vue`.
- B-9 **P2** Transport counter `33 / 33` offset from the board edge and different alignment in explore (centered) vs review. `board/Transport.vue`.
- B-10 **P2** Explore: truncated FEN/PGN text + two Copy buttons + textarea + `Play from here` + `OPEN A POSITION` kicker: 5 controls for 2 ideas (5.6). `panel/ExplorePanel.vue`, `panel/CopyRow.vue` (delete).
- B-11 **P2** Explore engine lines clip (`3. Nc3…`) with no hover/title on every line, start at inconsistent ply; PV not clickable. `panel/ExplorePanel.vue`.
- B-12 **P2** Judgement table lists six rows of zeros; colour-only labels (teal/blue/amber/orange/red) in a table with no glyph: colour-blind users cannot tell Mistake/Blunder from Inaccuracy. Add the glyph and show non-zero only. `panel/ReviewPanel.vue`, `board/Glyph.vue`.
- B-13 **P2** Missed chance label is jargon; `Brilliant`/`Great move` naming inconsistent with the move list glyphs. Copy (5.4).
- B-14 **P2** Move list rows are 34 px (`--e-tap` 36 on fine pointer minus), selected move is a cognac outline box on the second column only (`1. e4 e5` ... `9. h3 [h6]`); it looks like an input. Row 28 px, selected = `--c-sunk` + act bar. `board/MoveList.vue`, `board/MoveRow.vue`, `board/TreeMoves.vue`.
- B-15 **P2** Mistake glyph `??` appears next to the move in the list but with a different colour space than the table; consistent `--rv-*` (already) and use the same `Glyph` size 12 px.
- B-16 **P2** "That game isn't here" notice sits above the tabs in a bordered warning box that pushes the panel down 110 px. `UiEmpty`. `panel/Panel.vue` `.missing`.
- B-17 **P3** `Rematch` placement before the stats; belongs in an overflow. (5.4)
- B-18 **P3** Drill panel is a 200 px strip of text beside a full-height board. (5.7) `panel/DrillPanel.vue`.
- B-19 **P3** Watch panel's `The engine's line shows here while it thinks.` is permanent placeholder text. `panel/WatchPanel.vue`.
- B-20 **P3** Clocks: the active clock is a filled cognac box, the idle one a grey box; the grey box has lower contrast than 4.5:1 in light (`3:58` on `#e0d2c9`). `board/Strip.vue`.
- B-21 **P3** Board frame (inner shadow) differs between the stage and the sheet-docked board; verify one frame.

### R: Rail

- R-1 **P1** Real data: rail shows a single trial row, You filter says `No games here yet`, with 3,708 games, due to `limit=300` + client filter. Server-side filter + paging (5.2). `stores/games.ts`, `rail/GamesRail.vue`, `rail/rail.ts`, API query params (`GET /api/games` already supports `player`, `source`, `trial`).
- R-2 **P2** `1-0` coloured by raw result not by the user's outcome: `cutemouse83 - You 1-0` (you lost) shows red; `You - Dameless 1-0` green; `Dameless - Bot-Sam 1-0` green though you are not in it. `UiResult` (3.10). `rail/GameRow.vue`.
- R-3 **P2** Game titles use `-`, trials `vs`; mixed. `vs` everywhere. `rail/GameRow.vue`, `rail/GamesRail.vue`.
- R-4 **P2** Selected row is a full cognac outline box plus tinted fill: loudest object on screen. Act bar + `--c-sunk`.
- R-5 **P2** `live` word + dot duplicates; the word is red-brown mono. Dot only (plus `Live` tooltip). `rail/GameRow.vue`.
- R-6 **P2** Second line packs accuracy, clock, speed, rated: `97.0 · 96.0 · 3+0 · Blitz · rated` truncates to `10+5 · Rapid · rat…` at 288 px. Accuracy to a trailing tooltip; `3+0 Blitz · rated`.
- R-7 **P2** Filter segmented control is 5 options in 267 px with the selected one a cognac block; `Trials` label named `tests` in code (`RailFilter` `tests`): rename the value to `trials` to match vocabulary (and the ESegmented option). `rail/rail.ts`.
- R-8 **P2** Error state is a full-height pink panel (`error-1440-light.png`); `UiEmpty tone=error`.
- R-9 **P3** Empty state copy `Games you play, Dameless's games on Lichess and trial games all land here.` is a run-on; copy in 5.2.
- R-10 **P3** Trial caret rows have a 16 px caret gutter that non-trial rows render empty; align titles at one x.
- R-11 **P3** Relative time `20h`, `3w` fine; add a date separator for > 2 weeks (`Oct`), no per-row change.

### S: Sheets

- S-1 **P1** Every sheet title renders inside a rectangular focus box (it is auto-focused on open): looks like a text field (`sheet-*-1440-light.png`, top). `useSheet.ts`/`ESheet` heading focus; remove the visible outline for programmatic focus (`:focus:not(:focus-visible) { outline: none }` plus `UiSheetPage`).
- S-2 **P1** You, Dameless and Settings are 2,000 to 3,000 px single scrolls; no way to jump. Tabs (4.4, 5.8 to 5.13).
- S-3 **P1** You: filters (speed, rated) sit above the review bar and stats, scroll away, and have no label; the user cannot tell what they filter. Sticky toolbar at the top of the tab (5.8). `sheets/YouSheet.vue`, `parts/ProfileFilters.vue`, `parts/ProfileView.vue`.
- S-4 **P1** Sheet width 32rem is too narrow for tables (Openings header clipped; Levels table crowds; Compare is a 3-column table in 510 px). Wide variant (2.9, 3.14).
- S-5 **P2** Redundant descriptions under every title (`Your games added up…`, `The engine: its strength over…`, `Lichess, the machine, the theme and the hosted app.`, `A player on Lichess, from the games brought…`, `Players side by side over the games they have here, reviewed by the same engine.`). Delete.
- S-6 **P2** `Progress` chart is on both You and Dameless with different names (`ACCURACY AND RATING OVER TIME`, `PROGRESS`, `You and Dameless, side by side.`). Keep it on You only as `Progress` with the Dameless line toggle; Dameless links to it.
- S-7 **P2** Review bar is a tinted notice with a filled `Review all 23` plus an outlined `Import my Lichess games` stacked below; two competing buttons. One banner row, one primary (5.8). `parts/ReviewBar.vue` (the other agent is editing the count: only restyle the frame, keep its logic).
- S-8 **P2** Key figures: `Games 64 / 41 reviewed` and `Performance 1863 / Win streak 6` wrap onto a second grid row with inconsistent label/value gaps; the `?` after provisional ratings (`1345?`) reads as an error. `UiStat` (3.2).
- S-9 **P2** `Accuracy by phase and speed` merges two dimensions in one list (Opening, Middlegame, Endgame, Bullet, Blitz, Rapid, Classical) with identical bars. Two lists.
- S-10 **P2** `Moves per game` is a two-column table where the right half has no header alignment with the left (`Inaccuracy 4.32 177` aligned to the far right, `Brilliant 0.07 3` to the left); one table, two column pairs only on wide.
- S-11 **P2** Results rows: the W-D-L text (`34-5-25`) is printed in a tiny mono font under the bar and collides with the bar; score on the right at 18 px mono is larger than the bar. Row: label, bar with the record inside on hover, score. `parts/WdlBar.vue`, `parts/ProfileView.vue`.
- S-12 **P2** Chart axes: tick labels in ink-3 at 11 px with sparse x labels (`15 Aug`, `25 Aug`…) and a stray summary line below the chart (`27 Oct · accuracy 83.2 · rating 1813 · won`) that is the hover readout in rest state: make it appear only on hover, otherwise a legend row. `parts/LineChart.vue`, `parts/TrendChart.vue`, `parts/ProgressChart.vue`.
- S-13 **P2** Dameless Levels is a 13-row table with `about 2200` for unrated levels in the rating column and `0` games; `Level 6 NEAR YOUR RATING` stamp inside the cell. Ladder (5.9). `parts/LevelsCard.vue`.
- S-14 **P2** Logbook and Ideas in progress are two lists of the same item (`UiBadge`, title, evidence) styled differently (the logbook stamps are green KEPT/grey DROPPED boxes with a caret; in-progress are blue RUNNING with underline links). One `IdeaRow`. `parts/Logbook.vue`, `sheets/DamelessSheet.vue`.
- S-15 **P2** Idea sheet: hypothesis paragraph + stamp line + tag button-look + verdict sentence + Stop + `NOTES` + trials strip: the verdict is buried at the 4th line. Reorder (5.10). `sheets/IdeaSheet.vue`.
- S-16 **P2** Trial evidence: `Elo / nElo / Pairs` use `Term` underlines that look like links; `LLR +0.61` label sits on the gauge with `-2.94 · dropped` / `kept · +2.94` at the sides in mono 12 px; the gauge marker is a 2 px black bar hard to find. `parts/LlrGauge.vue`, `parts/TrialEvidence.vue`, `parts/PentaBar.vue`.
- S-17 **P2** Trial games list is underlined links (`v14 - deeper-lmr`) with ordinals only; no visible affordance difference from prose links. `UiTable` rows (5.10).
- S-18 **P2** Settings: 8 sections with uppercase mono labels (`ACCOUNT`, `LOGIN ENDS`, `CORES`, `ENGINE CHECKOUT`, `DATA`, `BINARIES`) and `READY` stamps on all 7 healthy checks. Tabs + `UiKeyValue` + failing-first (5.13). `sheets/SettingsSheet.vue` (617 lines).
- S-19 **P2** Compare: green cell fills for leaders (`Score, % 78.3`) make the column look selected; `Longest win streak` wraps; add-player at the bottom; the header cells have their own X buttons floating. (5.12) `sheets/CompareSheet.vue`, `sheets/compare.ts`.
- S-20 **P2** Player sheet when not imported: 4 rows on a 2,000 px empty sheet with a progress banner and an orphan `Compare with you` button. (5.11) `sheets/PlayerSheet.vue`.
- S-21 **P2** Challenges section shows `Accept Decline` as a cognac-outlined button + a link-looking `Decline`; `Cancel` outlined. All three are quiet buttons, Accept solid only if it is the only action. `parts/ChallengeList.vue`.
- S-22 **P3** Sheet header divider missing between header and content; first section starts 12 px under the title. `UiSheetPage`.
- S-23 **P3** `On Lichess` external link has no icon; external links get an arrow icon.
- S-24 **P3** Sheet back (when opened from another sheet) vs close: the 2-button header differs per sheet; one header (3.14).
- S-25 **P3** Brain: `Data used 1.0 of 10 GB` hatched meter `///` looks like a loading bar. Plain bar. `parts/BrainCard.vue`.
- S-26 **P3** Bot card paragraph `Rated and casual Bullet, Blitz and Rapid games…` is 4 lines of prose describing 6 settings; use `UiKeyValue` (5.9). `parts/BotCard.vue`.

### D: Dialogs

- D-1 **P1** Focus lands on the close X (visible heavy ring) in New game instead of the first field. `shell/NewGame.vue`, `UiDialog`.
- D-2 **P1** Primary left, Cancel right; Cancel is a white box that outweighs the primary. Cancel left, primary right (3.13).
- D-3 **P2** Uppercase mono kickers as field labels in every dialog (`OPPONENT`, `VERSION`, `KIND`, `TITLE`, `BRANCH`, `TAGS`, `POSITIONS`, `THREADS`, `NAME`, `SPEEDS`, `TAKES CHALLENGES`, `GAME LENGTH`). `UiField` (3.12).
- D-4 **P2** Switches used as options inside forms with Save (`Rated`, `Casual`, `From people`, `From bots`); checkboxes. `parts/BotSettingsDialog.vue`.
- D-5 **P2** Number steppers span half the dialog (50 px tall) for values like `0 s`; compact number fields. `parts/BotSettingsDialog.vue`, `parts/DatagenDialog.vue`.
- D-6 **P2** `Queue idea` shows a dashed outline when disabled and the reason (`Give it a title.`) sits beside Cancel. Standard disabled, reason on the field. `parts/NewIdea.vue`.
- D-7 **P2** Promote title wraps to 2 lines in a 30 px display style; `UiConfirm` (5.10). `parts/IdeaActions.vue`.
- D-8 **P2** Dialogs are centered modals at 375 with cut-off footers; bottom sheets (3.13). `shell/NewGame.vue`.
- D-9 **P3** `Either` for random colour; `Custom trial` jargon; `Keep playing` -> `Always`.
- D-10 **P3** Dialog footer background tint differs from the body (`#e0d2c9` vs surface) by a visible band that has no border; use `--c-raise` + hairline.
- D-11 **P3** Command bar rows with description lines for self-explanatory entries; trim (5.14).

### M: Mobile / responsive

- M-1 **P1** No navigation on phones; rail at the very bottom (28 rem fixed height) under a 30-row move list. Bottom tab bar (4.5).
- M-2 **P1** Move list on phones is unbounded (33 plies = 600 px) between the board and everything else. Horizontal strip + sheet (4.5).
- M-3 **P2** At 375 the header shows `You  Dameless` words with no figures and a `● 2` work count: unexplained. (5.1)
- M-4 **P2** 375 sheet: filters wrap into two full-width segmented rows (`All speeds | Bullet | Blitz | Rapid | Classical` at 11 px) that need horizontal scroll or font shrink; tab/overflow handling: `UiSeg` scrolls horizontally with a fade.
- M-5 **P2** Challenge row at 375 truncates `Rapid · 10+` (`Incoming · Rated · Rapid · 10+…`). Wrap to two lines or drop `Incoming`.
- M-6 **P3** Touch targets: rail filter options 28 px; move cells 34 px; icon buttons in Transport 36 px; coarse = 44 (2.6).
- M-7 **P3** `overflow` test: no horizontal overflow at 375/768/1280/1920 in all audited states (the audit's measured overflow was 0 everywhere). Keep it as a check in the final pass.

### Wording issues (collected)

`Rated and casual` -> `All` (filter); `Missed chance` -> `Missed win`; `Keep playing` -> `Always`; `Either` -> `Random`; `Custom trial` -> `Single trial`; `The bot's rules` -> `Bot rules`; `Offer a draw` -> `Offer draw`; `Average centipawn loss` -> `Avg. loss` (tooltip: average centipawn loss); `Centipawn loss` -> `Avg. loss`; `Rated and casual` in Compare likewise; `Games did not load` fine; `Nothing running` -> `Idle`; header `Dameless v14` fine.

---

## 7. Work split

The first agent must land before the others start (they can read this brief and write against the component API while waiting, but must not merge until Foundation is on main).

### Area A: Foundation (do first; blocks B to E)

Owns: `src/base.css`, `src/theme.ts`, `src/media.ts`, `src/format.ts` (`formatScore`, `formatResult`), `src/ui/**` (new, all components in section 3 plus `badge.ts`, `useConfirm.ts`), `tests/ui/**`, `src/parts/Term.vue`, `src/shell/Logo.vue`.

Deliver:

1. Tokens in 2 (spacing, type, colour, radii, density, layout, breakpoints), eos aliasing and per-control regression check at both schemes.
2. `src/ui/`: `UiSection`, `UiStat(s)`, `UiTable`, `UiKeyValue`, `UiEmpty`, `UiToolbar`, `UiPlayer`, `UiScore`, `UiBadge`, `UiResult`, `UiSeg`, `UiField`, `UiDialog`, `UiConfirm`, `UiSheetPage`, `UiPanelHead`, `UiLive`.
3. A hidden dev route `/?fixtures&kit` (route name `kit`, only in dev/fixtures) that renders every component in every variant, light and dark: the cohesion pass uses it as the reference.
4. Migration shims: keep `.sec/.figs/.part-title` until B to E finish; delete in the cohesion pass.

### Area B: Shell, rail, board stage

Owns: `src/App.vue`, `src/shell/*` (TopBar, StatusLine, NewGame, CommandBar, Shortcuts, new TabBar), `src/rail/*`, `src/stores/games.ts` (server-side filter + paging; coordinate query params with the API: if a param is missing, add to `crates/server` only with the user's okay and note it), `src/board/BoardStage.vue`, `Strip.vue`, `EvalBar.vue`, `EvalGraph.vue`, `Transport.vue`, `board.css`, `src/board/Board.vue` (frame only).

Covers: 5.1, 5.2, 4.1, 4.2, 4.5, issues B-1/3/6/7/8/9/20/21, R-*, M-1/3/6, D-1/8 (NewGame), T-12/13.

### Area C: Board panel and move list

Owns: `src/panel/*`, `src/board/MoveList.vue`, `MoveRow.vue`, `TreeMoves.vue`, `Glyph.vue`, `review.css`, `src/board/review.ts`, `drill.ts`, `src/shell/drillFlow.ts` (read only unless needed).

Covers: 4.3, 5.3 to 5.7, issues B-2/4/5/10 to 19, M-2 (move strip). Depends on A (`UiPanelHead`, `UiToolbar`, `UiSection`, `UiScore`, `UiEmpty`).

### Area D: You, Player, Compare (profiles)

Owns: `src/sheets/YouSheet.vue`, `PlayerSheet.vue`, `CompareSheet.vue`, `compare.ts`, `progress.ts`, `repertoire.ts`, `trend.ts`; `src/parts/Profile*.vue`, `ReviewBar.vue`, `WdlBar.vue`, `LineChart.vue`, `TrendChart.vue`, `ProgressChart.vue`, `RepertoireView.vue`, `DrillsCard.vue`, `ChallengeList.vue`, `AddPlayer.vue`, `TopPlayers.vue`, `ProfileFilters.vue`, `src/stores/players.ts`, `progress.ts`, `repertoire.ts`, `drills.ts`.

Covers: 5.8, 5.11, 5.12, issues S-2 to S-12, S-19 to S-21, M-4, M-5. **Note**: `ReviewBar.vue`, `ProfileFilters.vue`, `stores/players.ts` are being edited concurrently by another agent (review count fix, games-window filter). Rebase onto their commit before touching those three; restyle only (frame, `UiToolbar`, `UiSeg`) and keep their props, emits and logic.

### Area E: Dameless, Idea, Settings, dialogs

Owns: `src/sheets/DamelessSheet.vue`, `IdeaSheet.vue`, `SettingsSheet.vue`, `idea.ts`, `levels.ts`, `training.ts`, `tune.ts`, `useSheet.ts` (hand over to `UiSheetPage`); `src/parts/BotCard.vue`, `BotSettingsDialog.vue`, `bot.ts`, `BrainCard.vue`, `DatagenDialog.vue`, `ImportDatasetDialog.vue`, `LevelsCard.vue`, `Logbook.vue`, `IdeaActions.vue`, `NewIdea.vue`, `Notes.vue`, `LlrGauge.vue`, `PentaBar.vue`, `TrialEvidence.vue`, `TrainView.vue`, `TuneView.vue`, `markdown.ts`, `glossary.ts`; `src/stores/ideas.ts`, `dameless.ts`, `training.ts`, `versions.ts`, `work.ts`, `connections.ts`, `policy.ts`.

Covers: 5.9, 5.10, 5.13, 5.14 (all dialogs except New game, which is area B), issues S-13 to S-18, S-22 to S-26, D-2 to D-7, D-9 to D-11.

Rules for B to E: use only `src/ui/` components and tokens for sections, stats, tables, empties, badges, dialogs; if a component lacks something, add it to `src/ui/` via a small PR to Foundation's files (announce in the PR) rather than styling locally. Each area keeps `corepack pnpm lint`, `format:check`, `typecheck`, `test`, `knip` green; area B/D/E touch fixtures only where a state is missing (`newcomer`, `error`, `long`, `loading` exist; add `fairplay` states if absent). Each area re-shoots its screens at 375/768/1280/1440/1920 light+dark and updates `docs/ui-audit/` (git-ignored).

### Area F (final): Cohesion pass (one agent, after B to E)

Checks, in order:

1. Delete `.sec*`, `.figs*`, `.part-title`, `.plain`, `.sheet-actions`, `.toast-actions` if unused; knip clean; no bare rem/px/hex outside the exceptions list (grep).
2. One section header look, one stat, one table, one badge, one empty, one dialog frame everywhere (screenshot every screen; compare to `/?kit`).
3. Exactly one filled cognac button per view; list the violations. Cognac only as primary fill, selected marker, live dot, focus.
4. The board is the same size across Play, Watch, Review, Explore, Drill in the same window (measure `.cg-wrap` in each at 1280x800, 1440x900, 1920x1080; max delta 0 px).
5. All five widths x two schemes x every state: loading, empty, error, newcomer, long, fair-play. No horizontal overflow (the audit's `overflow` check: `document.documentElement.scrollWidth - innerWidth` = 0). No clipped controls (the 1280x800 Review).
6. Vocabulary scan of UI strings (forbidden words in AGENTS.md) and the copy table in section 6.
7. Contrast: body, muted, badge text, graph and judgement colours against both surfaces (4.5:1 text, 3:1 graphics).
8. Keyboard: tab order through header, rail, board, panel, sheet tabs; the sheet title no longer draws a box; Escape closes the topmost dialog, then the sheet; focus returns correctly.
9. Reduced motion, coarse pointer (44 px targets) at 375.
10. Real-app smoke test (read-only) against :7979: the rail shows your games, You loads, Dameless with 3 versions degrades cleanly.
11. `corepack pnpm api:check` untouched (no API changes unless area B adds the rail query; then `corepack pnpm api`).

---

## 8. Appendix: audit method, files, caveats

- Screenshots: headless Edge via playwright-core, `reducedMotion: reduce`, light and dark by `localStorage dameless.scheme`. Scripts are in the session scratchpad (`ui.mjs`, `batch-ui.mjs`, `b1.mjs` to `b7.mjs`); to regenerate, start `corepack pnpm dev --port 5460` in `apps/web` and run them with `BASE=http://localhost:5460`. The real app was only loaded and one filter button clicked (`real-rail-you-1440.png`): nothing acting.
- Index of screenshots (all in `C:\dev\chess\docs\ui-audit\`): `main-1440-{light,dark}`, `board-{play,watch,review,review-lichess,review-sf,review-import,explore,explore-game,drill}-1440-light`, `board-{play-375-light,play-768-light,play-1280-light,play-1920-light,review-375-dark,review-768-dark,review-1280-dark,review-1920-dark}`, `sheet-{you,dameless,idea1,idea2,idea3,player-cm,player-dameless,compare,settings}-1440-light`, `sheet-{dameless,you,settings,idea1}-tall-1440-light`, `sheet-you-375-light`, `sheet-dameless-768-light`, `sheet-compare-375-dark`, `sheet-settings-1280-dark`, `dlg-{newgame-1440-light,newgame-375-light,newgame-dark,command,shortcuts,newidea,botrules,datagen,importdata,promote,resign}-1440-light`, `empty-1440-light`, `error-1440-light`, `loading-1440-light`, `newcomer-you-1440-light`, `you-error-1440-light`, `settings-edge-1440-light`, `dameless-empty-1440-light`, `dameless-long-1440-light`, `real-{main,you,dameless,settings,rail-you}-1440`.
- Not fully covered (flag for the implementers): `?fixtures=fairplay` states, `botnoversion`, `untrusted`, `nogpu`, tune5000 and the Train/Tune views, challenge toast flows, promotion picker, 1920 dark sheets. Shoot them after your change and apply the same rules.
- Measured at 1440x900: header 49, footer 37, rail 288, stage 778, panel 374, board 647 (explore) / 593 (review) / 727 (drill). Tokens: control 44 px by eos default (40 px in `App.vue` fine pointer), gap scale 3/6/9/12/18.
- Decisions that deliberately depart from `docs/design.md` (update it in the cohesion pass): sheets are tabbed pages; the status footer is replaced by the header work indicator; mobile has a bottom tab bar; the rail is server-filtered and paged; single-tab tablists are hidden.

---

## 9. How to use the kit (written by Area A, Foundation)

The kit is built and on main. Read this before you touch a screen; it is how B to E use it the same way. See it at **`/?fixtures&kit`**: every component in every state, the scales, a measured contrast table, and what each `format.ts` function writes. It is its own app root, loaded only when fixtures are on (`main.ts`), so it is not in the app's main path. The scheme switch at its top flips light and dark.

### 9.1 The rules

1. Import by path, never a barrel: `import UiSection from "../ui/UiSection.vue"`. The components are `src/ui/Ui*.vue`; `UiKv` is a row of `UiKeyValue`; `badge.ts` has the status tones; `useConfirm.ts` the confirmation; `player.ts` the `PlayerRef` type.
2. **No screen styles its own section header, stat, table, empty state, badge, player name, result, score, segmented control, field label or dialog frame.** If a component lacks something, add it to `src/ui/` (a small PR, said so in its description) instead of styling locally.
3. Style only with tokens: `--sp-1..6`, `--t-*`, `--c-*`, `--r-*`, `--h-*`, `--i-*`, `--w-*` (all in `base.css`; the kit page draws each). No bare `rem`, `px` or hex, except 1 px hairlines through `--e-stroke-hair` and `--e-stroke-focus`. A font is `font: var(--t-body)`, never a size on its own. A figure is `--t-fig*` (mono); a name is sans.
4. Cognac (`--c-act`) is the one primary button, the selected marker, the live dot and focus. Never a link, a label or a decoration. Green and red are for results and judgements: `UiResult`, `UiScore perspective="mover"`, `UiBadge tone`.
5. Every state is designed: pass `loading` and `error` to `UiSheetPage` and `UiTable`, use `UiEmpty` for empties and failures, instead of writing a skeleton or a message of your own. For a skeleton the kit does not cover use the `.skel` class (it appears after 150 ms, so a fast answer never flashes one).
6. Text is sentence case. A missing number is `NONE` (`–`), or let `UiStat` and `UiTable` write it.

### 9.2 Old pattern, new component

| You have                                                    | Use                                                                                                                                        |
| ----------------------------------------------------------- | ------------------------------------------------------------------------------------------------------------------------------------------ |
| `.sec`, `.sec-head`, `.sec-note`, `<h3 class="part-title">` | `<UiSection title="…">` with `#aside` and `#note`; `:level="2"` for a part of a part; `collapsible` with `v-model:open`                    |
| `.figs`, `.fig`, `EMetric`                                  | `<UiStats><UiStat label value unit delta tone hint size sub /></UiStats>`; `hint="accuracy"` (a glossary key) replaces a `<Term>` label    |
| `EDataGrid`, `ETable`, a grid of divs                       | `<UiTable :columns :rows row-key label>` with `#cell-<key>="{ row, value }"`; a `@select` makes rows buttons                               |
| `EDescriptionList`, the `5+3 · local game` line             | `<UiKeyValue :items>` or `<UiKeyValue><UiKv label>…</UiKv></UiKeyValue>`; `inline` for the middle-dot line                                 |
| `EEmptyState`, `.sec-note` empties, `ENotice` as an empty   | `<UiEmpty title body>` with `#actions` (`dense` for the one-liner; `tone="error"` for a failed region with a Try again button)             |
| A row of filter buttons, `ESegmented`                       | `<UiSeg v-model label :options size full>`; `multiple` with `v-model:values`; `count` and `icon` per option                                |
| `EStamp`                                                    | `<UiBadge :tone>`; the tones of statuses are `ideaTone`, `trialTone`, `workTone`, `healthTone` in `ui/badge.ts`                            |
| Names with ratings (Strip, GameRow, headers, Top players)   | `<UiPlayer :seat colour size link live current :rating :provisional>`; a person who is not a seat is a `PlayerRef` `{ name, key, rating }` |
| `1-0` coloured by hand                                      | `<UiResult :result="resultOf(game.outcome)" :you="game.you">`; no `you` means no colour                                                    |
| A signed eval (`scoreText`)                                 | `<UiScore :score>`; `perspective="mover"` only in profile contexts                                                                         |
| `EField` and a kicker label                                 | `<UiField label hint error optional v-slot="{ id, describedby, invalid }">` around the control; `group` around a `UiSeg`                   |
| `EDialog`, `EConfirmDialog`                                 | `<UiDialog v-model:open title confirm-label @confirm>`; confirmations: `await useConfirm()({ title, body, confirmLabel, danger })`         |
| `ESheet` and `useSheet`                                     | `<UiSheetPage title subtitle :tabs v-model:tab>` (it does its own Close and Back)                                                          |
| The first lines of Play, Watch and Review panels            | `<UiPanelHead :game :mode>` with `#status` and `#actions`                                                                                  |
| Dots                                                        | `<UiLive tone pulse label>`                                                                                                                |
| Rows of buttons                                             | `<UiToolbar>` with `#end`                                                                                                                  |

### 9.3 Patterns

A sheet (this is the whole frame; there is no `ESheet`, `useSheet` or `.ws--sheet` to wire):

```vue
<UiSheetPage
  v-model:tab="tab"
  title="You"
  subtitle="dame_g · Lichess"
  :tabs="tabs"
  :loading="!profile"
  :error="failed"
  @retry="load"
>
  <template #actions><EButton variant="outline" size="sm">Review all</EButton></template>
  <template #lead><UiStats><UiStat label="Rating" :value="profile.rating" size="xl" /></UiStats></template>
  <template #tab-overview><UiSection title="Results">…</UiSection></template>
  <template #tab-games>…</template>
</UiSheetPage>
```

- `tabs` is `{ id, label, count? }[]`; one tab hides the tab row. The chosen tab is kept in the address as `?tab=` (`:sync="false"` turns that off). `#tab-<id>` is that tab's content; the default slot gets `{ tab }` if you prefer one slot.
- The title takes focus on open and draws no box. Close and Back return to the board as it was (what `useSheet` did) unless you listen for `@close` or `@back` yourself. Do not wrap it in anything else.
- It docks `position: absolute` to the right of the nearest positioned ancestor (`.ws`) at `--w-sheet` (`wide`: `--w-sheet-wide`) and covers the window below 75rem. **B**: make the workspace's third column `var(--w-sheet)` while a sheet is open (it is `--e-panel-width-md` today), and set `--sheet-inset-top` and `--sheet-inset-bottom` on the shell to what you keep visible on a phone (the tab bar's height). Both default to 0.

A dialog and a confirmation:

```vue
<UiDialog
  v-model:open="open"
  title="New idea"
  confirm-label="Queue idea"
  :busy="busy"
  :confirm-disabled="!title"
  @confirm="save"
>
  <UiField label="Title" v-slot="{ id }"><ETextField :id label="Title" hide-label v-model="title" /></UiField>
  <template #footer-start>{{ title ? "" : "Give it a title." }}</template>
</UiDialog>
```

```ts
const confirm = useConfirm();
if (await confirm({ title: "Resign this game?", body: "You lose the game.", confirmLabel: "Resign", danger: true }))
  resign();
```

- `UiDialog` never closes itself on confirm: you set `open` false when the work succeeds. Enter confirms from a field (Ctrl+Enter from a textarea); Escape and Cancel emit `cancel` and close; focus goes to the first field. Under 40rem it is a bottom sheet. `alert` is for a question that needs an answer (focus lands on Cancel).
- **B (once): mount `<UiConfirm />` once in `App.vue`**, next to `<NewGame />`. It is the host of every `useConfirm()`; without it a confirmation logs a warning and answers no.

A table:

```vue
<UiTable
  label="Levels"
  :columns="[
    { key: 'name', label: 'Level' },
    { key: 'rating', label: 'Rating', fig: true, sort: true, hide: 'sm' },
  ]"
  :rows="levels"
  row-key="level"
  @select="open"
>
  <template #cell-name="{ row }"><UiPlayer :seat="row.seat" /></template>
</UiTable>
```

- `fig` columns are mono, grouped, at the end edge; the first column is strong; words get the rest of the width, and a table that cannot fit scrolls inside itself with a fade. `hide: 'sm' | 'md'` hides a column when the **table's own** width is under 30rem or 48rem (a 32rem sheet hides `md` columns). `sticky` pins the head to the pane that scrolls (the table then does not scroll sideways: hide columns instead). `manual` with `sort` and `@sort` lets the parent sort (server paging).

### 9.4 Formatting (`format.ts`)

One function per kind of figure; use these in new code, and let a missing value be `NONE` (`–`). `formatNumber(n, digits)` (`3,708`), `formatElo(n, digits)` (`+12`, `−5`), `formatEloPm(elo, err)` (`+3.1 ± 2.0`), `formatPercent(n)` (`81.3%`), `formatRating(n, provisional)` (`1,345?`), `formatDuration(s)` (`45 s`, `1 h 20 min`), `formatRelative(when)` (`5 minutes ago`), `formatDate(when)` (`12 Oct`), `formatDateTime(when)` (for a tooltip), `formatScore(score)` (`+0.6`, `#3`), `scoreSide(score)`, `resultOf(outcome)`, `formatResult(result, you)`. The older `signed`, `scoreText`, `plusMinus`, `figure`, `percent`, `ago`, `shortDate` and `duration` stay for screens not yet migrated; move to the canonical ones as you go. The kit page lists what each writes.

### 9.5 What changed under you

- **Spacing is the 4 px grid.** eos's gaps and spaces are aliased onto `--sp-*` in `theme.ts` (after the theme's own tokens, so it wins), and eos paddings are tighter (`--e-pad-control` 12, `--e-pad-card` 12, `--e-pad-chip` 8). **Body is 14 px** (`--e-text-md` and `--e-text-sm` are 14, helper text is 12). **Controls are 32 px with a fine pointer, 44 px with a coarse one** (`--e-control-height` and `--e-tap`; `--h-control` for your own). **Buttons are flat**: `--e-depth` is 0, the slab under every button is gone, and the controls stay legible (see the kit). **One radius** (`--e-radius-sm` is `--e-radius-md`). eos links are ink with a quiet underline, glossary terms are dotted and carry no colour, `EField` labels are 12 px sentence case.
- `App.vue` still sets `--e-control-height: 2.5rem; --e-tap: 2.25rem` for fine pointers on `.shell`, which overrides the token below it: **B deletes that block** (T-6), with `--ws-pad` and `--line` (use `--sp-3` and `--c-line`).
- The legacy `.sec`, `.figs`, `.part-title`, `.plain`, `.sheet-actions` and `.toast-actions` classes still exist in `base.css` for screens not yet migrated. Do not use them in new code; the cohesion pass deletes them.
- Breakpoints: `src/media.ts` (`BREAKPOINTS`, `up("lg")`, `down("md")`, `useMedia`). A stylesheet writes the same numbers with the name in a comment.
- Tests: `tests/ui/` has a `mount()` helper (memory router, pinia) if you add to the kit.

### 9.6 Deviations from section 3 (decided while building; section 3 is otherwise as built)

- **`UiSeg` is homebrew, not a wrapper of `ESegmented`.** `ESegmented` allows two to five options and its markup fights the quiet style; `UiSeg` takes any number, shows `count`, has `multiple` (`v-model:values`), a roving-tabindex radio group with arrow keys, Home and End, a reserved bold width so choosing never shifts a neighbour, and a 44 px target on a coarse pointer (`size="sm"` is 28 px fine, 44 coarse).
- **`UiSheetPage` is homebrew, not `ESheet`**, which cannot give a sticky tab row, a title without a focus box, or the `--w-sheet` widths. It does Close and Back itself (`useSheetNav`, which replaces `useSheet`); `contained` is for previews; `tab` is `v-model:tab`; it emits `close`, `back` and `retry`.
- **`UiDialog` wraps `EDialog`** (focus trap, portal, Escape) and restyles it. `open` is `v-model:open`; it adds `alert`, and `confirm-label` makes the default primary (the `#actions` slot replaces both buttons). **`UiConfirm` is the host component** of `useConfirm()`: a composable alone has nowhere to render.
- `UiStat`: `digits` for a numeric value; each stat is its own `<dl>`. `UiTable`: `label`, `manual`, `sort`, `expanded` and `edge` added; `hide` is by container width; row selection is detected by a `@select` listener. `UiKeyValue`: `UiKv` rows, `inline`, and items may carry a `key` for `#value-<key>`. `UiPlayer`: `current`, `PlayerRef`, and the tooltip is the full name. `UiScore`: `turn` (for `mate: 0`). `UiField`: a scoped slot `{ id, describedby, invalid }` and `group`. `UiSection`: `headingLevel`. `UiToolbar`: `label`. `UiLive`: `pulse` and `label`. `UiPanelHead`: draws nothing without a game, and says how a finished game ended when it has no `#status`.
- The kit is a query toggle (`?kit` with `?fixtures`) that swaps the app root in `main.ts`, not a router route named `kit`.
