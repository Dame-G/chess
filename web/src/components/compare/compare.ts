import type { GlossaryKey } from "../../glossary";
import type { CompareQuery, Moment, PlayerStats } from "../../api/queries";

export const SPEEDS = ["bullet", "blitz", "rapid", "classical"] as const;
export const LIMITS = [20, 50, 100, 200] as const;
const DEFAULT_LIMIT = 50;
/** Fewer reviewed games than this and a column's numbers are an impression, not a measure. */
const FEW_GAMES = 10;
/** Fewer than this and no row is led by it. */
const LEADS_FROM = 3;

export type Classification = Moment["classification"];

/** The move classes in the order they are shown, best first, with Lichess's glyphs and which way is better. */
export const classes: readonly {
  key: Classification;
  label: string;
  glyph: string;
  better?: "high" | "low";
}[] = [
  { key: "brilliant", label: "Brilliant", glyph: "!!", better: "high" },
  { key: "great", label: "Great", glyph: "!", better: "high" },
  { key: "best", label: "Best", glyph: "★", better: "high" },
  { key: "excellent", label: "Excellent", glyph: "✓", better: "high" },
  { key: "good", label: "Good", glyph: "•" },
  { key: "book", label: "Book", glyph: "≡" },
  { key: "inaccuracy", label: "Inaccuracy", glyph: "?!", better: "low" },
  { key: "mistake", label: "Mistake", glyph: "?", better: "low" },
  { key: "blunder", label: "Blunder", glyph: "??", better: "low" },
  { key: "miss", label: "Miss", glyph: "✕", better: "low" },
];

export const glyphOf = (c: Classification) => classes.find((k) => k.key === c)?.glyph ?? "";

// ---- The address: who is compared, over which games.

const NAME = /^[A-Za-z0-9_-]{2,30}$/;

/** What the address says. `me` is always first; names are Lichess usernames, once each. */
export function readState(query: Record<string, unknown>): CompareQuery {
  const text = (v: unknown) => (typeof v === "string" ? v : "");
  const seen = new Set<string>(["me"]);
  const players = ["me"];
  for (const name of text(query.players).split(",")) {
    const n = name.trim();
    const key = n.toLowerCase();
    if (key === "bot" ? seen.has("bot") : !NAME.test(n) || seen.has(key)) continue;
    seen.add(key);
    players.push(key === "bot" ? "bot" : n);
  }
  const speeds = SPEEDS.filter((s) => text(query.speeds).split(",").includes(s));
  const limit = Number(text(query.limit));
  return {
    players,
    speeds: speeds.length === SPEEDS.length ? [] : speeds,
    rated: text(query.rated) === "1",
    limit: (LIMITS as readonly number[]).includes(limit) ? limit : DEFAULT_LIMIT,
  };
}

/** The address for a state; what is the default is left out. */
export function stateQuery(state: CompareQuery): Record<string, string | undefined> {
  return {
    players: state.players.length > 1 ? state.players.join(",") : undefined,
    speeds: state.speeds.length ? state.speeds.join(",") : undefined,
    rated: state.rated ? "1" : undefined,
    limit: state.limit === DEFAULT_LIMIT ? undefined : String(state.limit),
  };
}

// ---- Words and numbers.

const dash = "–";

/** One decimal, or a dash when there is nothing to say. */
const fixed = (n: number | null | undefined, digits = 1) => (n == null ? dash : n.toFixed(digits));

const percent = (n: number | null | undefined) => (n == null ? dash : `${n.toFixed(1)}%`);

/** Seconds a move took, for people: "3.9 s". */
const seconds = (ms: number | null | undefined) => (ms == null ? dash : `${(ms / 1000).toFixed(1)} s`);

/** The name a column goes by. */
export function nameOf(p: Pick<PlayerStats, "player" | "username">): string {
  return p.username ?? (p.player === "me" ? "You" : p.player === "bot" ? "Your bot" : p.player);
}

export type Sample = "none" | "few" | "ok";

/** How much a column's engine numbers rest on. */
export const sampleOf = (p: Pick<PlayerStats, "reviewed">): Sample =>
  p.reviewed === 0 ? "none" : p.reviewed < FEW_GAMES ? "few" : "ok";

/** The move number and san as a player reads them: `14. Ng6+`, `13… Bxf3`. */
export function moveLabel(m: Pick<Moment, "ply" | "side" | "san">): string {
  return `${Math.ceil(m.ply / 2)}${m.side === "white" ? "." : "…"} ${m.san}`;
}

/** Where a moment is on the board. */
export const momentPath = (m: Pick<Moment, "match_id" | "ply">) => `/matches/${m.match_id}?ply=${m.ply}`;

/** About how long reviewing takes on this machine: some 7 seconds a game on three engines. */
function reviewMinutes(games: number): number {
  return Math.max(1, Math.round((games * 7) / 60));
}

export const reviewCost = (games: number) =>
  `A review takes about 20 s of CPU per game, so ${games} ${games === 1 ? "game takes" : "games take"} roughly ${reviewMinutes(games)} ${reviewMinutes(games) === 1 ? "minute" : "minutes"} in the background.`;

// ---- The sheet.

export interface Cell {
  text: string;
  /** Beneath the text, smaller. */
  sub?: string;
  /** A bar under the text, 0 to 1. */
  bar?: number;
  /** What the row's leader is judged on. */
  value?: number | null;
  lead?: boolean;
  /** The stacked move-quality bar. */
  mix?: { key: Classification; share: number }[];
}

interface Row {
  id: string;
  label: string;
  term?: GlossaryKey;
  glyph?: Classification;
  /** Indented under the row before it. */
  nested?: boolean;
  better?: "high" | "low";
  cells: Cell[];
}

interface Group {
  id: string;
  title: string;
  hint?: string;
  rows: Row[];
}

export interface Sheet {
  groups: Group[];
  /** For each column, the rows it leads, and how many rows had a leader to win. */
  leads: number[];
  contested: number;
}

const speedLabel = (s: string) => s.charAt(0).toUpperCase() + s.slice(1);

/** Marks the best cell of a row, when there is a real contest: two numbers that differ, from columns that rest on enough games. */
function markLeader(row: Row, columns: readonly PlayerStats[], engine: boolean): boolean {
  if (!row.better) return false;
  const eligible = row.cells
    .map((c, i) => ({ c, i }))
    .filter(({ c, i }) => c.value != null && (!engine || columns[i]!.reviewed >= LEADS_FROM));
  if (eligible.length < 2) return false;
  const values = eligible.map(({ c }) => c.value!);
  const best = row.better === "high" ? Math.max(...values) : Math.min(...values);
  if (values.every((v) => v === best)) return false;
  for (const { c } of eligible) c.lead = c.value === best;
  return true;
}

/** Builds the rows of the stat sheet from the players, in the order of the columns. */
export function buildSheet(players: readonly PlayerStats[]): Sheet {
  const row = (
    id: string,
    label: string,
    cell: (p: PlayerStats) => Cell,
    extra: Partial<Omit<Row, "id" | "label" | "cells">> = {},
  ): Row => ({ id, label, cells: players.map(cell), ...extra });
  /** A cell that only exists when the player has reviewed games. */
  const engine = (make: (p: PlayerStats) => Cell) => (p: PlayerStats) =>
    p.reviewed > 0 ? make(p) : { text: dash, value: null };
  const ratio = (v: number | null | undefined): Cell => ({
    text: percent(v),
    value: v ?? null,
    bar: v == null ? undefined : Math.max(0, Math.min(1, v / 100)),
  });

  const speedsPresent = SPEEDS.filter((s) => players.some((p) => p.by_speed.some((b) => b.speed === s && b.reviewed)));
  const accuracy: Group = {
    id: "accuracy",
    title: "Accuracy",
    rows: [
      row(
        "acc",
        "Overall",
        engine((p) => ratio(p.accuracy.overall)),
        { term: "accuracy", better: "high" },
      ),
      row(
        "acc-opening",
        "Opening",
        engine((p) => ratio(p.accuracy.opening)),
        { nested: true, better: "high" },
      ),
      row(
        "acc-middlegame",
        "Middlegame",
        engine((p) => ratio(p.accuracy.middlegame)),
        {
          nested: true,
          better: "high",
        },
      ),
      row(
        "acc-endgame",
        "Endgame",
        engine((p) => ratio(p.accuracy.endgame)),
        { nested: true, better: "high" },
      ),
      ...speedsPresent.map((s) =>
        row(
          `acc-${s}`,
          speedLabel(s),
          (p) => {
            const b = p.by_speed.find((x) => x.speed === s);
            return b?.reviewed ? { ...ratio(b.accuracy), sub: `${b.reviewed} games` } : { text: dash, value: null };
          },
          { nested: true, better: "high" },
        ),
      ),
      row(
        "acpl",
        "Centipawn loss",
        engine((p) => ({ text: fixed(p.acpl, 0), value: p.acpl ?? null })),
        {
          term: "acpl",
          better: "low",
        },
      ),
    ],
  };

  const present = classes.filter((c) => c.key !== "book" || players.some((p) => p.counts.book > 0));
  const quality: Group = {
    id: "quality",
    title: "Moves per game",
    hint: "total in grey",
    rows: [
      row(
        "mix",
        "Mix",
        engine((p) => {
          const total = classes.reduce((n, c) => n + p.counts[c.key], 0);
          return {
            text: "",
            mix: total ? classes.map((c) => ({ key: c.key, share: p.counts[c.key] / total })) : [],
          };
        }),
      ),
      ...present.map((c) =>
        row(
          `q-${c.key}`,
          c.label,
          engine((p) => ({
            text: p.per_game[c.key].toFixed(p.per_game[c.key] < 10 ? 2 : 1),
            sub: String(p.counts[c.key]),
            value: p.per_game[c.key],
          })),
          { term: c.key, glyph: c.key, better: c.better },
        ),
      ),
    ],
  };

  const wdl = (id: string, label: string, pick: (p: PlayerStats) => PlayerStats["results"]["overall"]) =>
    row(
      id,
      label,
      (p) => {
        const r = pick(p);
        return r.games
          ? { text: percent(r.score), sub: `${r.wins}–${r.draws}–${r.losses}`, value: r.score ?? null }
          : { text: dash, value: null };
      },
      { better: "high", nested: id !== "res" },
    );
  const results: Group = {
    id: "results",
    title: "Results",
    hint: "score, then wins–draws–losses",
    rows: [
      wdl("res", "Score", (p) => p.results.overall),
      wdl("res-white", "As White", (p) => p.results.white),
      wdl("res-black", "As Black", (p) => p.results.black),
      row(
        "perf",
        "Performance",
        (p) => ({ text: fixed(p.opponents.performance, 0), value: p.opponents.performance ?? null }),
        {
          term: "performance",
          better: "high",
        },
      ),
      row("opp", "Opponents", (p) => ({ text: fixed(p.opponents.average_rating, 0) }), {}),
      row(
        "streak",
        "Longest win streak",
        (p) => ({ text: p.games ? String(p.longest_win_streak) : dash, value: p.games ? p.longest_win_streak : null }),
        {
          better: "high",
        },
      ),
    ],
  };

  const time: Group = {
    id: "time",
    title: "Time per move",
    rows: [
      row(
        "t-avg",
        "Average",
        engine((p) => ({ text: seconds(p.time.average_ms) })),
      ),
      row(
        "t-opening",
        "Opening",
        engine((p) => ({ text: seconds(p.time.opening_ms) })),
        { nested: true },
      ),
      row(
        "t-middlegame",
        "Middlegame",
        engine((p) => ({ text: seconds(p.time.middlegame_ms) })),
        { nested: true },
      ),
      row(
        "t-endgame",
        "Endgame",
        engine((p) => ({ text: seconds(p.time.endgame_ms) })),
        { nested: true },
      ),
      row(
        "t-trouble",
        "Time-trouble blunders",
        engine((p) => ({
          text: p.time.time_trouble_blunders_per_game.toFixed(2),
          sub: String(p.time.time_trouble_blunders),
          value: p.time.time_trouble_blunders_per_game,
        })),
        { term: "timeTrouble", better: "low" },
      ),
    ],
  };

  const depth = Math.min(5, Math.max(0, ...players.map((p) => p.openings.length)));
  const openings: Group = {
    id: "openings",
    title: "Most played openings",
    hint: "games, score",
    rows: Array.from({ length: depth }, (_, i) =>
      row(`o-${i}`, String(i + 1), (p) => {
        const o = p.openings[i];
        return o ? { text: o.name, sub: `${o.results.games} · ${percent(o.results.score)}` } : { text: dash };
      }),
    ),
  };

  const groups = [accuracy, quality, results, time, openings].filter((g) => g.rows.length);
  const leads = players.map(() => 0);
  let contested = 0;
  for (const g of groups)
    for (const r of g.rows) {
      if (!markLeader(r, players, g.id !== "results")) continue;
      contested++;
      r.cells.forEach((c, i) => {
        if (c.lead) leads[i]!++;
      });
    }
  return { groups, leads, contested };
}
