# UI brief

How the web app (`apps/web`) looks and how it is built. The app plays games (you against an engine from the engines list, or on Lichess), reads them (review, the eval graph, the better move), and adds them up (your profile, a comparison, drills). `docs/design.md` › The web app lists the screens and routes; this file is the look: tokens, the kit, the layout and the rules, so a new screen draws like the rest.

Vocabulary rules from AGENTS.md hold (you, engine, analysis engine, level, game, player, work, review, connection). Never in UI copy: build, match, experiment, test, batch, job, runner, human.

See the kit at `/?fixtures&kit` (every component in every state, the scales, a measured contrast table). Every screen can be walked without the app through the fixtures: `?fixtures` (or `?fixtures=empty|error|loading|newcomer|long|settings-edge|fairplay`), which the dev server answers from `src/api/fixtures*.ts`.

---

## 0. Direction

Saddle palette (cognac, brass, linen / espresso), radius 3, flat controls, hairline dividers: **quiet and ranked**.

- One loud colour use per screen: the primary action. A selected state is ink on a tint with an underline, not a solid slab.
- Outcome colour (win green, loss red) is only for results and judgements.
- Everything numeric is mono, right-aligned and tabular; everything else is sans, sentence case.
- Sheets are tabbed pages with a real header (title, subtitle as data, actions) and a sticky tab row.
- Phones get a bottom tab bar (Board, Games, You).
- A small kit in `src/ui/` draws every section header, stat, table, empty state, badge and dialog frame: no screen styles its own.

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

Body text is 14 px app-wide. The `--e-type-body` the eos shell uses stays for eos internals; set `body { font: var(--t-body) }`.

Delete: `.part-title` (replaced by `UiSection` header), `0.8125rem` everywhere, uppercase letter-spaced mono kickers in forms and tables (`type-kicker` as a label). eos `EField` labels: override to `--t-meta` muted sentence case (`:where(.e-field-label)`), 500.

Section label (the one allowed uppercase): not used. Section titles are `--t-head` sentence case, ink, with a trailing hairline. This is what makes the hierarchy: titles are now actually bigger and darker than content.

Mono rule: only figures, move notation (SAN/UCI/FEN), clocks, versions/branch names. Names (players, openings, engines) are sans.

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

- **Filled cognac** = one primary button per view (and `Play`, `Save`, `Add engine` in dialogs). Nothing else is filled cognac. Selected segments become: `background: var(--c-sunk)`, `color: var(--c-ink)`, `font-weight: 600`, plus a 2 px `--c-act` bar under the label. (Implement as `UiSeg` variant, section 3.)
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

Elevation: none inside panes. Dialogs/popovers use eos `--e-shadow-float`; the board keeps its current frame. Remove `depth`-based stripes from eos buttons: `--e-depth` stays 1px but buttons get no bottom slab (the "snappy slab" under every button reads as chunky next to a dense table).

### 2.5 Dividers

Pane-to-pane (rail | stage | panel): one `1px var(--c-line)`. Section-to-section in a sheet: the section's top hairline plus `--sp-5` above and `--sp-3` below the line. Rows in lists: hairline between, none after the last.

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

Set `--e-control-height: 2rem; --e-tap: 2rem` under `@media (pointer: fine)` in `base.css` and `2.75rem` under coarse. Tokens: `--h-control`, `--h-row`, `--h-row-2`, `--h-table-row`, `--h-move`, `--h-tab` for the rest, defined once.

### 2.7 Icons

Phosphor (already used). Sizes: 14 (inline with `--t-meta`), 16 (default, in buttons and rows), 20 (header, tab bar), 28 (empty states). One weight (regular); bold only for the check/close in a selected state. Delete `--e-icon-size-*` overrides in components. Icon-only buttons always have `aria-label` and a tooltip on fine pointers.

### 2.8 Motion

Keep eos snappy. Add: `@media (prefers-reduced-motion: reduce)` kills sheet slide, the live-dot pulse and the loading shimmer (already partly). New: the live dot pulses once per move (not continuously).

### 2.9 Layout tokens

```css
:root {
  --w-rail: clamp(16rem, 18vw, 20rem); /* 256..320 */
  --w-panel: clamp(22rem, 24vw, 28rem); /* 352..448 */
  --w-sheet: clamp(32rem, 34vw, 44rem); /* sheets dock here beside the board */
  --w-sheet-wide: clamp(40rem, 52vw, 64rem); /* Compare and You > Openings tables */
  --w-measure: 62ch;
}
```

---

## 3. Homebrew components (`src/ui/`)

One `.vue` per component in `src/ui/`, imported by path (there is no barrel). Prefix `Ui`. All are slotted, token-only, no scoped hard-coded sizes, with tests in `tests/ui/` for the logic bits (UiTable alignment, UiResult mapping, UiScore sign/colour).

### 3.1 `UiSection.vue`

The one section frame.

```
props: title: string; level?: 1 | 2 (default 1); collapsible?: boolean; open?: boolean (v-model); id?: string
slots: default; #aside (right side of the header: filters, a link, a button); #note (one muted line under the title)
```

Renders `<section>` with a header row: title (`--t-head`) left, `#aside` right (wraps below the title under 30rem), then `#note` in `--t-meta`, then content with `--sp-2` gap. Level 1 has the top hairline and `--sp-5` margin from the previous section; level 2 is the same without the hairline and `--t-body-strong`.

### 3.2 `UiStat.vue` and `UiStats.vue`

```
UiStats props: columns?: number (default auto-fit min 7rem); dense?: boolean
UiStat props: label: string; value: string | number | null; unit?: string; delta?: number | string; tone?: 'win'|'loss'|'warn'|'info'; hint?: string (tooltip + glossary key); size?: 'md'|'lg'|'xl'; sub?: string
slots: default (custom value), #label (to add a Term), #sub
```

Label above in `--t-meta` muted (a `Term` dotted underline only if `hint`), value below in `--t-fig-lg` (or xl for the screen's hero), unit in `--t-meta` after the value, `sub` below in `--t-meta`. `null` value renders an en dash in `--c-ink-3`, never "0" or "?" literal (real data shows `1345?` and `Accuracy: -`).

### 3.3 `UiTable.vue` (DataTable)

For every table in the app: levels, openings, moves-per-game, compare, top players, repertoire.

```
props: columns: { key: string; label: string; align?: 'start'|'end'|'center'; width?: string; fig?: boolean (mono tabular); sort?: boolean; hide?: 'sm' | 'md' (hide below breakpoint) }[]
       rows: T[]; rowKey: keyof T | (r) => string; dense?: boolean (default true); selected?: string; sticky?: boolean (header sticks inside its scroll parent)
       empty?: string; loading?: boolean (skeleton rows)
slots: #cell-<key>="{ row, value }"; #head-<key>; #foot; #row-aside (expand)
emits: select(row), sort(key, dir)
```

28 px rows, hairline separators, header 24 px `--c-raise` sentence-case `--t-meta` 500 (**not** mono uppercase), numeric columns right-aligned `--t-fig`, first column `--t-body-strong`, zero borders at the sides, horizontal scroll inside with a fade edge on overflow (fixes the Openings table whose header text is clipped at 510 px). Selected row `--c-sunk` + 2 px act bar at the start edge. Hover `--c-raise`.

### 3.4 `UiKeyValue.vue`

```
props: items?: { label: string; value: string|number; mono?: boolean; copy?: boolean; tone? }[]; columns?: 1|2; dense?: boolean
slots: #value-<key> | default (<UiKeyValue><UiKv label="Cores">15</UiKv></UiKeyValue>)
```

Label left (`--t-meta`, 9rem fixed on desktop, stacked above on mobile), value right-flowing `--t-body`, mono only if the value is a path/commit/number. Row height `--h-row`, hairline between.

### 3.5 `UiEmpty.vue`

```
props: icon?: Component; title: string; body?: string; tone?: 'neutral'|'error'; dense?: boolean (inline in a section: one line, no icon)
slots: #actions (max one primary + one quiet)
```

Centered in its container with `--sp-6` top padding, 28 px icon in `--c-ink-3`, title `--t-head`, body `--t-body` muted max 36ch, actions row. `dense` = one muted line with an inline link button (replaces `.sec-note`). `tone="error"` has a danger icon, no red panel.

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

Renders: colour pip (square 10 px white/black outlined; the seat's side), name (`--t-body-strong`; "You" is the user's account name only in profile contexts), `BOT`/`FM`/`NM` (a Lichess player's title) as a `UiBadge` (neutral, tiny), rating in `--t-fig` muted with `?` shown as `1345?` only when `provisional`.

### 3.8 `UiScore.vue` (ScoreBadge)

```
props: score: Score (cp | mate) | number (pawns); perspective?: 'white'|'mover'; size?: 'sm'|'md'
```

`+0.6` / `-4.2` / `#3`: mono, sign always shown, one decimal, mate as `#n`. Colour: **no colour** by default (white advantage is not "good"); a small square pip of the side ahead sits left of it (white or black fill). Tone only when `perspective="mover"` (profile contexts).

### 3.9 `UiBadge.vue` (Stamp)

```
props: tone?: 'neutral'|'win'|'loss'|'warn'|'info'|'act'; solid?: boolean; dot?: boolean
```

18 px high, `--t-meta` 600 sentence case ("Running", "Kept", "Near your rating", "Connected"), 1.5 px inner padding, radius `--r-1`, tinted fill (12% tone) with tone-coloured text, **no border, no mono, no uppercase**. `solid` only for LIVE.

### 3.10 `UiResult.vue` (ResultMark)

```
props: result: '1-0'|'0-1'|'1/2-1/2'|'*'|null; you?: 'white'|'black'|null; format?: 'score'|'letter'|'dot'
```

From the user's perspective when `you` is set: win = green `1-0`/`W`, loss = red, draw = muted `½-½`; without `you` (other people's games) always muted ink. Fixes the rail where `1-0` is green for every game regardless of who the user was (cutemouse83 - You `1-0` is a **loss** and shows red-ink `1-0` by accident of colour, "You - knightowl 0-1" shows red: inconsistent). Fixed-width (3 characters), mono `--t-fig`, right-aligned.

### 3.11 `UiSeg.vue` (segmented control, quiet)

Wrap `ESegmented` rather than reimplement; style via `:deep`.

```
props: modelValue; options: { value; label; icon?; count? }[]; size?: 'sm'|'md'; label: string (a11y); full?: boolean
```

No borders between options; track `--c-sunk`, selected option `--c-pane` with ink text 600 and 2 px act underline. `count` as `--t-fig` muted. Single source for: rail filter, speed, rated/casual, As White/As Black, time control, opponent, engine (New game's first step), colour, theme (System/Light/Dark).

### 3.12 `UiField.vue`

```
props: label: string; hint?: string; error?: string; optional?: boolean; for?: string
slots: default (the control)
```

Label (`--t-meta` 500, sentence case, "Optional" appended in muted instead of right-aligned), control, hint (`--t-meta`) or error (`--t-meta` danger). 4 px label gap, 12 px between fields.

### 3.13 `UiDialog.vue`

```
props: open (v-model); title: string; size?: 'sm'(26rem)|'md'(32rem)|'lg'(40rem); busy?: boolean; danger?: boolean
slots: default; #footer-start (hints, validation like "Give it a title."); #actions (override);
emits: confirm; cancel
props for actions: confirmLabel: string; cancelLabel?: string (default Cancel); confirmDisabled?: boolean
```

Frame: title `--t-title` (single line, ellipsis), body `--sp-4`, sticky footer on `--c-raise` with **Cancel left, primary right**, Enter confirms, Escape cancels, focus goes to the first field (not the close button; fix: the New game dialog opens with a focus ring on the X). Close button 32 px quiet icon. Below 40rem the dialog is a bottom sheet (full width, 85dvh, handle bar). `danger` makes the confirm button red.

### 3.14 `UiSheetPage.vue`

The frame every sheet uses (replaces direct `ESheet` + `useSheet` binding in 6 sheets).

```
props: title: string; subtitle?: string (data, e.g. "dame_g · Lichess"); tabs?: { id; label; count? }[]; tab?: string (v-model, synced to route hash/query ?tab=); wide?: boolean; loading?: boolean; error?: string|null
slots: #actions (header right: external link, primary button); #lead (the hero strip under the header, always visible above tabs); default (tab content, scope: tab id); #tab-<id>
```

Behaviour: header 48 px (back chevron if applicable, title, actions, close X) is **sticky**; hero strip (`#lead`, up to 2 rows of `UiStat`) scrolls away; tab row sticky below header, 36 px, `UiSeg`-style underline tabs; content scrolls. Title is `<h1>` with `tabindex="-1"` and `outline: none` (focus moves there for screen readers but no box is drawn: see issue S-1). Sheet width `--w-sheet`, `wide` -> `--w-sheet-wide`. Below 75rem it is full-window, with a back arrow instead of X. Loading shows the hero + tab skeleton at final geometry. Error shows `UiEmpty tone="error"` with Try again.

### 3.15 `UiPanelHead.vue` (the game card at the top of the board panel)

```
props: game: GameSummary | null; mode: Mode
slots: #status (what is happening now), #actions
```

Two lines: `UiPlayer white  [UiResult]  UiPlayer black`, then `Opening · 5+3 · rated · Lichess` in `--t-meta`. This is the one place the panel states who is playing whom; the board strips keep clock + name but the panel no longer repeats "Stockfish played h6. Your move." plus "5+3 · local game". Replaces the first lines of Play/Watch/Review panels.

### 3.16 `UiLive.vue`

8 px dot, `--c-act` (live) or `--c-win` (connected) or `--c-loss` (error), optional pulse. Replaces dots in the rail, status line, header work indicator, Watch.

### 3.17 Utilities in `base.css`

`.num`, `.mono` stay. Delete `.sec*`, `.figs*`, `.fig*`, `.part-title`, `.plain`, `.sheet-actions` after migrating. Add `.stack` (grid gap `--sp-4`), `.row` (flex gap `--sp-2` wrap), `.muted`, `.truncate`, `.sr-only` if eos lacks it.

---

---

## 4. Layout

### 4.1 Shell

Header, rail, board stage, panel or sheet.

```
>= 75rem (1200)
+---------------------------------------------------------------------------------------------+
| Chess      [ Search or run a command  Ctrl K ]     [+ New game]  [Train 5]  You 1,840  [work] [gear] |  48
+----------+------------------------------------------------------------+---------------------+
| Games    |                      stage (board)                         | panel / sheet       |
| rail     |                                                            |                     |
| 256-320  |                                                            | 352-448 / 512-704   |
+----------+------------------------------------------------------------+---------------------+
```

- **Header**: the wordmark, the command bar, then New game (a quiet outline: the filled button of a view is that view's own), Train (when drills wait), `You 1,840`, the work indicator (the one place background work appears) and Settings. Below 48rem it keeps the wordmark glyph, the search, New game and the icons.
- **Rail** `--w-rail`: the games list; **stage**: the board; **panel** `--w-panel`; sheets `--w-sheet` (or wide). Hairlines `--c-line` between regions; the rail and panel are `--c-pane`, the stage `--c-page`.
- Breakpoints are one list in `src/media.ts` (`sm` 30rem, `md` 48rem, `lg` 75rem, `xl` 100rem), referenced by name.

| Width                | Layout                                                                                                                                                                                       |
| -------------------- | -------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| < 48rem (phone)      | One column: the board edge to edge, the panel under it, a bottom tab bar (Board, Games, You). The rail is the Games tab, full screen. Sheets are full-screen pages with the tab bar visible. |
| 48 to 75rem (tablet) | Board (sized by height) left, panel right, the rail behind a header button that slides over. A sheet covers the window under the header.                                                     |
| 75rem and up         | Rail, stage and panel or sheet side by side. The board is capped by height, so extra width at 1920 is margin, not a bigger board.                                                            |

### 4.2 The board stage

The board never changes size or position when the mode, game or panel changes: the chrome around it is always reserved (`--strip` 36 for each player strip, `--graph` 56 for the eval graph or an empty baseline, `--transport` 32). In Explore and Drill the strips show the side to move and the opening, and the graph slot is empty. The eval bar is 16 px wide, full board height, its figure a `UiScore` chip in the strip; phones have no bar and the same chip in the strip. The graph is neutral (one zero line, the side ahead in a 14 % ink area, judged moves as dots in `--rv-*`).

### 4.3 The panel

Top to bottom, the same in every mode: `UiPanelHead` (the game card; nothing in free Explore), mode tabs only when there is more than one mode (a finished game: Review and Explore), the mode's content (at most about 40 % of the panel's height, so it never pushes the move list off screen), and the move list, which fills the rest and scrolls inside.

### 4.4 Sheets

A sheet is a page that docks beside the board (`UiSheetPage`): a sticky header (title, subtitle as data, actions, close), an optional lead strip, tabs, content. The stage shrinks (the board re-fits and stays live); under 75rem a sheet is full-screen with a back arrow. Tabs per sheet: You = Overview, Openings, Train, Challenges (only when there are any); Settings = Accounts, Engines, This machine, Health, Appearance; Player and Compare are one screen.

### 4.5 Phones

The bottom tab bar (`--h-tabbar` plus the safe area) has Board, Games and You; Settings stays in the header with the work indicator. The Board tab's move list is one horizontal row under the transport with Moves opening the full list in a bottom sheet.

---

## 5. The screens

### 5.1 Games rail

One list, filtered All, You, Lichess, with live games pinned on top (a pulsing dot, no word), then newest first under month headings; the filter and the paging are the server's. A row is 48 px: `UiPlayer` white `vs` `UiPlayer` black (names `--t-body-strong`, ellipsis), then the trailing `UiResult` (from your side) or the live dot; the second line is `--t-meta`: `10+5 · Rapid · rated` and the relative time, with a muted `97 / 96` when reviewed. The selected row is `--c-sunk` with a 2 px act bar. Nothing yet is one sentence and one button (New game); an error is `UiEmpty tone="error"` with Try again.

### 5.2 Play

A game you sit in: the panel head, a status line (`Your move`, `Stockfish is thinking`: the turn, not the last move), a toolbar (Offer draw, Take back quiet; Resign quiet-danger, which confirms), then the moves. Take back exists only for a game with no clock, or a person on Lichess who may say no. On Lichess nothing else is on screen (no eval bar, no engine, no graph) and the work indicator says why.

### 5.3 Review

A finished game: the head (players, result, how it ended, opening), for a local game against an engine the **end card** (how it went, the rating context, the one filled **Rematch**, the level one step easier and harder as outline buttons, "Review this game" until asked), then the **review summary**: accuracy per side (the screen's one hero figure), average loss, the judgement chips (non-zero classes only; each jumps to the next such move), a hairline line naming the engine and its budget (`Reviewed by Stockfish 17.1`, `200,000 nodes a move`), the current-move card (the glyph, the eval, `Better: Qb3` with Line), and **Play from here** with the panel menu (copy FEN or PGN). Not reviewed: `UiEmpty dense` and one button, Review it, with the time it takes; a running review shows progress with Pause; fair play shows `Analysis is off while you play on Lichess.`

### 5.4 Watch and Explore

**Watch** (a live game you are not in): the head with a live dot, the thinking engine's line when there is one (a `UiScore` and one line of PV), the moves. **Explore** (free analysis from any position): a toolbar (Engine switch, depth, Play from here, Copy FEN or PGN), three engine lines (score right-aligned, the PV in mono with ellipsis, a click plays it onto the board), a one-line paste field for a FEN or PGN, the explorer, the tablebase and your repertoire, then the tree. A Lichess game of yours turns the whole panel into `UiEmpty`: `Analysis is off while you play on Lichess.` with Back to my game.

### 5.5 Drill

One of your drills takes the board and the panel: the prompt (`White to move: find the move you missed`), the opening and phase as a muted line, progress as a thin bar, `[Hint] [Skip]`. After the answer, a result card (`Correct`, `Not this one`) with the best line and Next as the one filled button.

### 5.6 You

A tabbed wide sheet. The lead is your ratings (the speed with most games as the hero, the others beside it). **Overview**: filters in a toolbar (speed, rated or casual, how many games) applied to everything, the figures (games, score, accuracy, average loss, performance, win streak), the review notice (`23 games are not reviewed yet: Review all 23 · Check for new games`), Progress (rating, accuracy and drill hit rate over time), results by colour, accuracy by phase and by speed, moves per game by class, time per move, best moments and worst blunders. **Openings**: the openings you play and the repertoire tree. **Train**: today's drills and Start, the hit rate, themes, weak spots with Play this out, and the **Levels** ladder of an engine (a `UiSeg` of the engines with levels, the analysis engine first; each level an interval and a dot on one shared scale; calibrate that engine with its cost). **Challenges** when there are any. Newcomers (no games) see one `UiEmpty`: play one, or bring in your Lichess history.

### 5.7 Player and Compare

**Player**: someone else's profile from the games brought in here: nothing imported yet is the whole sheet (`Import 50 games`); then the same profile with Compare with you as the header action; imports are off while you play on Lichess. **Compare**: a wide sheet, a toolbar (speed, newest games, Add a player at the top), a `UiTable` with a sticky first column and header, row groups as subheaders, the leader of a row in bold with a dot (ties both), and a note that rows marked count reviewed games only.

### 5.8 Settings

Five tabs. **Accounts**: your Lichess login (connected state, when it ends, Log out; not connected is one sentence and Log in with Lichess). **Engines**: one row for each engine: its name, badges (Analysis, Plays at a rating it was given, Added, Not installed), what it reports in small text (what it calls itself, or where it is; that its levels are node budgets alone when it has no MultiPV), and its actions at the end edge (Use for analysis, Remove for yours); Add engine opens a dialog whose server errors show at the field they are about (a taken name at Name, a program that does not answer as UCI at Program). **Setup** below it: what is left to download as rows (name, its host and file in small text, its size), a Total, and the one filled button `Download 3.9 MB`; nothing left is one sentence. **This machine**: cores and the data directory (paths ellipsize in the middle, with a copy button). **Health**: the checks as a compact list (a dot, a name, one line; failures first, each with its fix and a Copy). **Appearance**: theme, sounds and volume, and the hosted app's token.

### 5.9 Dialogs

All use `UiDialog` (sizes `sm` for confirms, `md` for forms); confirmations are `useConfirm()`: the action as a question, one sentence of consequence, Cancel left, the action right (danger when it destroys).

**New game**: Opponent as a `UiSeg` (Engine, Lichess, Lichess AI). For an engine, two steps: **Engine** (a `UiSeg` of engines: the analysis engine first, then Reckless and each engine you added) with one line under it for what it is, then **Strength** (that engine's levels with its ratings for them and the one nearest yours marked) as one radio list that scrolls to the chosen row. An engine that plays as it is has no second step. Arrow keys move through both; each family remembers the strength last chosen in it; the whole choice is remembered (`new.engine`, the seat the server reads: `stockfish@5`, `reckless@3`). What is not installed is listed but cannot be chosen, and a line under the steps links to Settings › Engines. Then Time control (presets, No clock, Custom), You play (White, Random, Black), and for Lichess Find a game or Challenge someone. The primary says what it starts: `Play 10+5 · Stockfish 6`, `Play 5+3 · Maia 1500`, `Find a game`. At 375 it is a bottom sheet.

**Add engine**: Name, Program (the executable on this machine, any UCI engine), UCI options one per line as `Name=value`; errors at the field.

**Command bar** (`Ctrl K`): sections Places, Do, Games as `--t-meta` headers, shortcut chips as `Kbd`, the verbs in the CLI's words (`play 5+3 stockfish`, `review last 20`). **Shortcuts**: a grouped two-column reference.

### 5.10 Global states

- **Loading**: skeletons in final geometry (rail rows, the panel head, a sheet's lead and tabs), after 150 ms; no spinners except inside buttons.
- **Error**: the page keeps working; a failed region shows `UiEmpty tone="error"` with Try again. Losing the connection shows one thin banner under the header (`Can't reach the app. Reconnecting…`), not an error per region.
- **Toasts**: sentence case, no trailing period under five words.

---

## 6. How to use the kit

The kit is on main; read this before you touch a screen.

### 6.1 The rules

1. Import by path, never a barrel: `import UiSection from "../ui/UiSection.vue"`. The components are `src/ui/Ui*.vue`; `UiKv` is a row of `UiKeyValue`; `badge.ts` has the status tones; `useConfirm.ts` the confirmation; `player.ts` the `PlayerRef` type.
2. **No screen styles its own section header, stat, table, empty state, badge, player name, result, score, segmented control, field label or dialog frame.** If a component lacks something, add it to `src/ui/` (said so in the PR) instead of styling locally.
3. Style only with tokens: `--sp-1..6`, `--t-*`, `--c-*`, `--r-*`, `--h-*`, `--i-*`, `--w-*` (all in `base.css`). No bare `rem`, `px` or hex, except 1 px hairlines through `--e-stroke-hair` and `--e-stroke-focus`. A font is `font: var(--t-body)`, never a size on its own. A figure is `--t-fig*` (mono); a name is sans.
4. Cognac (`--c-act`) is the one primary button, the selected marker, the live dot and focus. Never a link, a label or a decoration. Green and red are for results and judgements: `UiResult`, `UiScore perspective="mover"`, `UiBadge tone`.
5. Every state is designed: pass `loading` and `error` to `UiSheetPage` and `UiTable`, use `UiEmpty` for empties and failures. For a skeleton the kit does not cover use the `.skel` class (it appears after 150 ms, so a fast answer never flashes one).
6. Text is sentence case. A missing number is `NONE` (`–`), or let `UiStat` and `UiTable` write it.

### 6.2 What to use

| You need                            | Use                                                                                                                                        |
| ----------------------------------- | ------------------------------------------------------------------------------------------------------------------------------------------ |
| A section                           | `<UiSection title="…">` with `#aside` and `#note`; `:level="2"` for a part of a part; `collapsible` with `v-model:open`                    |
| Figures                             | `<UiStats><UiStat label value unit delta tone hint size sub /></UiStats>`; `hint="accuracy"` (a glossary key) makes the label a `Term`     |
| A table                             | `<UiTable :columns :rows row-key label>` with `#cell-<key>="{ row, value }"`; a `@select` makes rows buttons                               |
| Labels and values                   | `<UiKeyValue :items>` or `<UiKeyValue><UiKv label>…</UiKv></UiKeyValue>`; `inline` for the middle-dot line                                 |
| An empty or failed region           | `<UiEmpty title body>` with `#actions` (`dense` for the one-liner; `tone="error"` with a Try again button)                                 |
| Filters, options, a choice of a few | `<UiSeg v-model label :options size full>`; `multiple` with `v-model:values`; `count` and `icon` per option; `UiSelect` for a dropdown     |
| A status as a word                  | `<UiBadge :tone>`; the tones of statuses are in `ui/badge.ts`                                                                              |
| A player with a rating              | `<UiPlayer :seat colour size link live current :rating :provisional>`; a person who is not a seat is a `PlayerRef` `{ name, key, rating }` |
| A result                            | `<UiResult :result="resultOf(game.outcome)" :you="game.you">`; no `you` means no colour                                                    |
| A signed eval                       | `<UiScore :score>`; `perspective="mover"` only in profile contexts                                                                         |
| A form field                        | `<UiField label hint error optional v-slot="{ id, describedby, invalid }">`; `group` around a `UiSeg`                                      |
| A dialog or a confirmation          | `<UiDialog v-model:open title confirm-label @confirm>`; `await useConfirm()({ title, body, confirmLabel, danger })`                        |
| A sheet                             | `<UiSheetPage title subtitle :tabs v-model:tab>` (it does its own Close and Back)                                                          |
| The top of the board panel          | `<UiPanelHead :game :mode>` with `#status` and `#actions`                                                                                  |
| A dot, a row of buttons             | `<UiLive tone pulse label>`, `<UiToolbar>` with `#end`                                                                                     |

### 6.3 Patterns

A sheet is the whole frame (no wrapper needed):

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
</UiSheetPage>
```

`tabs` is `{ id, label, count? }[]`; one tab hides the tab row; the chosen tab is kept in the address as `?tab=` (`:sync="false"` turns that off). A dialog and a confirmation:

```vue
<UiDialog
  v-model:open="open"
  title="Add an engine"
  confirm-label="Add engine"
  :busy="busy"
  :confirm-disabled="!name"
  @confirm="save"
>
  <UiField label="Name" v-slot="{ id }"><ETextField :id label="Name" hide-label v-model="name" /></UiField>
</UiDialog>
```

```ts
const confirm = useConfirm();
if (await confirm({ title: "Resign this game?", body: "You lose the game.", confirmLabel: "Resign", danger: true }))
  resign();
```

`UiDialog` never closes itself on confirm: set `open` false when the work succeeds. Enter confirms from a field; Escape and Cancel close; under 40rem it is a bottom sheet; `alert` is for a question that needs an answer. `<UiConfirm />` is mounted once in `App.vue` and hosts every `useConfirm()`.

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

`fig` columns are mono and at the end edge; `hide: 'sm' | 'md'` hides a column when the table's own width is under 30rem or 48rem; `sticky` pins the head; `manual` with `sort` and `@sort` lets the parent sort.

### 6.4 Formatting (`format.ts`)

One function per kind of figure; a missing value is `NONE` (`–`). `formatNumber(n, digits)` (`3,708`), `formatElo(n, digits)` (`+12`, `−5`), `formatEloPm(elo, err)` (`+3.1 ± 2.0`), `formatPercent(n)` (`81.3%`), `formatRating(n, provisional)` (`1,345?`), `formatDuration(s)` (`45 s`, `1 h 20 min`), `formatRelative(when)`, `formatDate(when)`, `formatDateTime(when)` (a tooltip), `formatScore(score)` (`+0.6`, `#3`), `scoreSide(score)`, `resultOf(outcome)`, `formatResult(result, you)`. The kit page lists what each writes.

### 6.5 What the kit changes under eos

- **Spacing is the 4 px grid**: eos's gaps and spaces are aliased onto `--sp-*` in `theme.ts`. **Body is 14 px**; **controls are 32 px with a fine pointer, 44 px with a coarse one**; **buttons are flat** (`--e-depth` 0); **one radius**; eos links are ink with a quiet underline; `EField` labels are 12 px sentence case.
- `UiSeg` is homebrew (any number of options, `count`, `multiple`, a roving-tabindex radio group, a reserved bold width so choosing never shifts a neighbour). `UiSheetPage` is homebrew (a sticky tab row, a title without a focus box, the `--w-sheet` widths; `contained` is for previews). `UiDialog` wraps `EDialog` (focus trap, portal, Escape) and restyles it. `UiSelect` is the one dropdown (`dense` for a toolbar).
- Breakpoints: `src/media.ts` (`BREAKPOINTS`, `up("lg")`, `down("md")`, `useMedia`); a stylesheet writes the same numbers with the name in a comment. Tests: `tests/ui/` has a `mount()` helper (memory router, pinia) for the kit.
