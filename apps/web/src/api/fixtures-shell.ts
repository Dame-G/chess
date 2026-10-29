/**
 * Fixture variants for the shell and the Games rail: states the default data never reaches, and the games list as the
 * server answers it (filtered, paged after a game, live first). `?fixtures=empty` has no games and no work,
 * `?fixtures=many` has hundreds of games with long names, `?fixtures=flood` is the real app's shape (thousands of trial
 * games newer than thousands of imported ones, so a rail that filters what it was handed finds nothing of yours),
 * `?fixtures=slow` answers the games late (the skeleton) and `?fixtures=error` fails to load them.
 */
import type { GameSummary, Seat } from "./types";
import { error, reply } from "./fixtures-http";
import { fixtureMode } from "./mode";

const LONG = [
  "an-exceptionally-long-lichess-username-42",
  "Dameless:deeper-late-move-reductions-with-history",
  "xX_the_Knight_Whisperer_Xx",
  "Grandmaster-Of-Very-Long-Names",
];

/** Games cloned from the real ones, with their own ids, long names, and a spread of ages. */
function manyGames(base: GameSummary[]): GameSummary[] {
  return Array.from({ length: 300 }, (_, i) => {
    const g = structuredClone(base[i % base.length]!);
    g.id = 1000 + i;
    g.started_at = new Date(Date.now() - (i + 1) * 47 * 60_000 * (1 + (i % 7))).toISOString();
    if (g.status === "live") g.status = "finished";
    if (i % 3 === 0) g.black = { ...g.black, name: LONG[i % LONG.length]! };
    if (i % 5 === 0) g.white = { ...g.white, name: LONG[(i + 1) % LONG.length]! };
    return g;
  });
}

const seat = (name: string, player: string, rating: number | null = null): Seat => ({ name, player, rating });

let floodCache: GameSummary[] | null = null;
/**
 * The real app's shape: 4,000 trial games from the last hours, then 3,708 of your Lichess games over the last years,
 * all newer than nothing in the default data. Built once.
 */
function floodGames(): GameSummary[] {
  if (floodCache) return floodCache;
  const now = Date.now();
  const out: GameSummary[] = [];
  for (let i = 0; i < 4000; i++) {
    out.push({
      id: 100_000 + i,
      source: { type: "trial", trial: 3, index: i },
      white: seat(i % 2 ? "v14" : "deeper-lmr", "dameless"),
      black: seat(i % 2 ? "deeper-lmr" : "v14", "dameless"),
      time: { type: "clock", initial_ms: 11_040, increment_ms: 110 },
      status: "finished",
      outcome: (["white_wins", "draw", "black_wins"] as const)[i % 3]!,
      plies: 80,
      started_at: new Date(now - 60_000 - i * 20_000).toISOString(),
    });
  }
  const speeds = ["blitz", "rapid", "bullet", "classical"] as const;
  for (let i = 0; i < 3708; i++) {
    const white = i % 2 === 0;
    out.push({
      id: 200_000 + i,
      source: { type: "import", id: `imp${i}`, owner: "you", rated: i % 4 !== 0, speed: speeds[i % 4]! },
      white: white
        ? seat("dame_g", "you", 1840)
        : seat(`opponent${i % 97}`, `lichess:opponent${i % 97}`, 1500 + (i % 400)),
      black: white
        ? seat(`opponent${i % 97}`, `lichess:opponent${i % 97}`, 1500 + (i % 400))
        : seat("dame_g", "you", 1840),
      time: {
        type: "clock",
        initial_ms: [180, 600, 60, 900][i % 4]! * 1000,
        increment_ms: [2, 0, 0, 10][i % 4]! * 1000,
      },
      status: "finished",
      outcome: (["white_wins", "draw", "black_wins"] as const)[(i * 7) % 3]!,
      plies: 60 + (i % 40),
      started_at: new Date(now - 3 * 86_400_000 - i * 3_600_000 * 5).toISOString(),
      you: white ? "white" : "black",
      ...(i % 5 === 0 ? { accuracy: { white: 70 + (i % 25), black: 72 + (i % 20) } } : {}),
    });
  }
  floodCache = out;
  return out;
}

const live = (g: GameSummary) => (g.status === "live" || g.status === "starting" ? 1 : 0);
const newestFirst = (a: GameSummary, b: GameSummary) =>
  live(b) - live(a) || b.started_at.localeCompare(a.started_at) || b.id - a.id;

/** `GET /api/games` as the server answers it: filtered, live first then newest, the page after `before`, `limit` long. */
export function queryGames(all: readonly GameSummary[], params: URLSearchParams): GameSummary[] {
  const list = (name: string) => params.get(name)?.split(",") ?? null;
  const player = params.get("player");
  const sources = list("source");
  const statuses = list("status");
  const trial = params.get("trial");
  const before = params.get("before");
  const limit = Number(params.get("limit") ?? 50);
  let rows = all
    .filter((g) => !player || g.white.player === player || g.black.player === player)
    .filter((g) => !sources || sources.includes(g.source.type))
    .filter((g) => !statuses || statuses.includes(g.status))
    .filter((g) => trial === null || (g.source.type === "trial" && g.source.trial === Number(trial)))
    .sort(newestFirst);
  if (before !== null) {
    const at = rows.findIndex((g) => g.id === Number(before));
    rows = at < 0 ? [] : rows.slice(at + 1);
  }
  return rows.slice(0, limit);
}

/** The answer a variant gives for a route, or `null` when the normal fixtures answer. */
export async function shellVariant(
  method: string,
  path: string,
  url: URL,
  games: () => GameSummary[],
): Promise<Response | null> {
  const modes = (fixtureMode() ?? "").split("+");
  const mode = (name: string) => modes.includes(name);
  if (method !== "GET") return null;
  if (path === "/api/games") {
    if (mode("empty")) return reply("/api/games", "get", []);
    if (mode("error")) return error(500, "internal", "The app did not answer.");
    if (mode("slow")) await new Promise((r) => setTimeout(r, 6000));
    const base = games();
    const all = mode("many") ? [...base, ...manyGames(base)] : mode("flood") ? [...base, ...floodGames()] : base;
    return reply("/api/games", "get", queryGames(all, url.searchParams));
  }
  if (path === "/api/work" && mode("empty")) return reply("/api/work", "get", { running: [], queued: [], recent: [] });
  return null;
}
