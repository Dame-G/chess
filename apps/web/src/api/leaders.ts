import type { Leader, Profile } from "./types";

type Pick = (p: Profile) => number | null | undefined;

/** The rows that have a leader, as `core::profile::leaders` defines them: `true` is higher-is-better. */
const ROWS: [Leader["row"], Pick, boolean][] = [
  ["score", (p) => p.results.overall.score, true],
  ["accuracy", (p) => p.accuracy.overall, true],
  ["opening", (p) => p.accuracy.opening, true],
  ["middlegame", (p) => p.accuracy.middlegame, true],
  ["endgame", (p) => p.accuracy.endgame, true],
  ["acpl", (p) => p.acpl, false],
  ["blunders", (p) => (p.reviewed > 0 ? p.counts_per_game.blunder : null), false],
  ["mistakes", (p) => (p.reviewed > 0 ? p.counts_per_game.mistake : null), false],
  ["brilliant", (p) => (p.reviewed > 0 ? p.counts_per_game.brilliant : null), true],
  ["performance", (p) => p.opponents.performance, true],
  ["streak", (p) => (p.games > 0 ? p.longest_win_streak : null), true],
  ["time_trouble", (p) => (p.time.games > 0 ? p.time.time_trouble_blunders_per_game : null), false],
];

/**
 * Who leads each row. Ties lead together; a row only one player has figures for leads nobody. The server computes
 * this for real (`GET /api/compare`); the fixtures use this copy.
 */
export function leaders(profiles: readonly Profile[]): Leader[] {
  return ROWS.map(([row, pick, higher]) => {
    const values = profiles.map((p) => pick(p) ?? null);
    const present = values.filter((v): v is number => v !== null);
    if (present.length < 2) return { row, players: [] };
    const best = present.reduce((a, b) => (higher ? Math.max(a, b) : Math.min(a, b)));
    const players = values.flatMap((v, i) => (v !== null && Math.abs(v - best) < 1e-9 ? [i] : []));
    return { row, players };
  });
}
