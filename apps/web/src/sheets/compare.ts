import type { Comparison, LeaderRow, Profile, Speed } from "../api/types";
import { speedName } from "../format";
import type { TermKey } from "../parts/glossary";

/** The players a comparison starts with. */
const DEFAULT_PLAYERS = ["you", "dameless"];
const DEFAULT_LIMIT = 200;
export const LIMITS = [50, 100, 200, 500];
/** The speeds a profile can be filtered by. */
export const SPEEDS: Speed[] = ["bullet", "blitz", "rapid", "classical"];
const MAX_PLAYERS = 5;

/** What the address of `/compare` holds: who, and which games count. */
export interface CompareAddress {
  players: string[];
  speeds: Speed[];
  rated: boolean | null;
  limit: number;
}

type Query = Record<string, string | (string | null)[] | null | undefined>;
const first = (v: string | (string | null)[] | null | undefined): string =>
  Array.isArray(v) ? (v[0] ?? "") : (v ?? "");

/** Whether a string is a player key the server knows how to answer: you, dameless, lichess:<name>, stockfish:<1-8>. */
export function isPlayerKey(key: string): boolean {
  return /^(you|dameless|lichess:[a-z0-9][a-z0-9_-]{0,28}[a-z0-9]|stockfish:[1-8])$/.test(key);
}

/** Reads `?players=you,dameless,lichess:x&speeds=blitz&rated=true&limit=100`, ignoring what it cannot use. */
export function parseAddress(query: Query): CompareAddress {
  const players: string[] = [];
  for (const raw of first(query["players"]).split(",")) {
    const key = raw.trim().toLowerCase();
    if (isPlayerKey(key) && !players.includes(key)) players.push(key);
  }
  const speeds = first(query["speeds"]).split(",");
  const rated = first(query["rated"]);
  const limit = Number(first(query["limit"]));
  return {
    players: (players.length ? players : DEFAULT_PLAYERS).slice(0, MAX_PLAYERS),
    speeds: SPEEDS.filter((s) => speeds.includes(s)),
    rated: rated === "true" ? true : rated === "false" ? false : null,
    limit: Number.isInteger(limit) && limit > 0 ? Math.min(limit, 1000) : DEFAULT_LIMIT,
  };
}

/** The address' query, leaving out what is the default so a plain comparison has a short link. */
export function addressQuery(a: CompareAddress): Record<string, string> {
  const q: Record<string, string> = { players: a.players.join(",") };
  if (a.speeds.length) q["speeds"] = a.speeds.join(",");
  if (a.rated !== null) q["rated"] = String(a.rated);
  if (a.limit !== DEFAULT_LIMIT) q["limit"] = String(a.limit);
  return q;
}

/**
 * A player key from what people type or paste: a Lichess name, `@name`, `lichess:name`, or a link to a profile
 * (`https://lichess.org/@/name`). `null` when it cannot be a Lichess name.
 */
export function playerFromInput(text: string): string | null {
  let t = text.trim();
  if (!t) return null;
  if (/^(you|dameless)$/i.test(t)) return t.toLowerCase();
  const link = t.match(/lichess\.org\/@\/([^/?#\s]+)/i);
  if (link) t = link[1]!;
  t = t.replace(/^lichess:/i, "").replace(/^@/, "");
  const key = `lichess:${t.toLowerCase()}`;
  return isPlayerKey(key) ? key : null;
}

/** The Lichess name inside a key, else the key. */
export const nameOfKey = (key: string): string => key.replace(/^lichess:/, "");

// ---- Rows

interface RowSpec {
  id: string;
  label: string;
  term?: TermKey;
  /** The row of `Comparison.leaders` that tints the leader's cell. */
  leader?: LeaderRow;
  get: (p: Profile) => number | null;
  /** The figure as shown. */
  text: (v: number) => string;
}

export interface GroupSpec {
  id: string;
  title: string;
  rows: RowSpec[];
}

const one = (v: number) => v.toFixed(1);
const two = (v: number) => v.toFixed(2);
const whole = (v: number) => String(Math.round(v));
const seconds = (v: number) => `${(v / 1000).toFixed(1)} s`;

/** The rows of a comparison, grouped. Per-speed accuracy rows appear for the speeds some profile has played. */
export function compareGroups(profiles: readonly Profile[]): GroupSpec[] {
  const speeds = SPEEDS.filter((s) => profiles.some((p) => p.accuracy.by_speed.some((x) => x.speed === s)));
  const speedRow = (s: Speed): RowSpec => ({
    id: `acc-${s}`,
    label: speedName[s],
    get: (p) => p.accuracy.by_speed.find((x) => x.speed === s)?.accuracy ?? null,
    text: one,
  });
  const perGame = (id: keyof Profile["counts_per_game"], label: string, leader?: LeaderRow): RowSpec => ({
    id: `pg-${id}`,
    label,
    ...(leader ? { leader } : {}),
    get: (p) => (p.reviewed > 0 ? p.counts_per_game[id] : null),
    text: two,
  });
  return [
    {
      id: "results",
      title: "Results",
      rows: [
        { id: "games", label: "Games", get: (p) => p.games, text: whole },
        { id: "score", label: "Score, %", leader: "score", get: (p) => p.results.overall.score, text: one },
        { id: "white", label: "As White, %", get: (p) => p.results.white.score, text: one },
        { id: "black", label: "As Black, %", get: (p) => p.results.black.score, text: one },
        {
          id: "performance",
          label: "Performance",
          term: "performance",
          leader: "performance",
          get: (p) => p.opponents.performance,
          text: whole,
        },
        {
          id: "streak",
          label: "Longest win streak",
          leader: "streak",
          get: (p) => (p.games ? p.longest_win_streak : null),
          text: whole,
        },
      ],
    },
    {
      id: "accuracy",
      title: "Accuracy",
      rows: [
        {
          id: "acc",
          label: "Overall",
          term: "accuracy",
          leader: "accuracy",
          get: (p) => p.accuracy.overall,
          text: one,
        },
        { id: "acc-opening", label: "Opening", leader: "opening", get: (p) => p.accuracy.opening, text: one },
        {
          id: "acc-middlegame",
          label: "Middlegame",
          leader: "middlegame",
          get: (p) => p.accuracy.middlegame,
          text: one,
        },
        { id: "acc-endgame", label: "Endgame", leader: "endgame", get: (p) => p.accuracy.endgame, text: one },
        ...speeds.map(speedRow),
        { id: "acpl", label: "Centipawn loss", term: "acpl", leader: "acpl", get: (p) => p.acpl, text: one },
      ],
    },
    {
      id: "moves",
      title: "Moves per game",
      rows: [
        perGame("brilliant", "Brilliant", "brilliant"),
        perGame("great", "Great"),
        perGame("inaccuracy", "Inaccuracies"),
        perGame("mistake", "Mistakes", "mistakes"),
        perGame("blunder", "Blunders", "blunders"),
        perGame("miss", "Missed chances"),
      ],
    },
    {
      id: "time",
      title: "Time per move",
      rows: [
        { id: "t-avg", label: "Average", get: (p) => p.time.average_ms, text: seconds },
        { id: "t-opening", label: "Opening", get: (p) => p.time.opening_ms, text: seconds },
        { id: "t-middlegame", label: "Middlegame", get: (p) => p.time.middlegame_ms, text: seconds },
        { id: "t-endgame", label: "Endgame", get: (p) => p.time.endgame_ms, text: seconds },
        {
          id: "t-trouble",
          label: "Time-trouble blunders per game",
          leader: "time_trouble",
          get: (p) => (p.time.games ? p.time.time_trouble_blunders_per_game : null),
          text: two,
        },
      ],
    },
  ];
}

/** Whether profile `index` leads `row` in a comparison. */
export function leads(cmp: Pick<Comparison, "leaders">, row: LeaderRow | undefined, index: number): boolean {
  return !!row && !!cmp.leaders.find((l) => l.row === row)?.players.includes(index);
}
