/**
 * Fixture variants for the shell and the Games rail: states the default data never reaches. `?fixtures=empty` has no
 * games and no work, `?fixtures=many` has hundreds of games with long names, `?fixtures=slow` answers the games late
 * (the skeleton) and `?fixtures=error` fails to load them.
 */
import type { GameSummary } from "./types";
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

/** The answer a variant gives for a route, or `null` when the normal fixtures answer. */
export async function shellVariant(method: string, path: string, games: () => GameSummary[]): Promise<Response | null> {
  const mode = fixtureMode();
  if (method !== "GET") return null;
  if (path === "/api/games") {
    if (mode === "empty") return reply("/api/games", "get", []);
    if (mode === "error") return error(500, "internal", "The app did not answer.");
    if (mode === "many") return reply("/api/games", "get", [...games(), ...manyGames(games())]);
    if (mode === "slow") {
      await new Promise((r) => setTimeout(r, 6000));
      return null;
    }
  }
  if (path === "/api/work" && mode === "empty")
    return reply("/api/work", "get", { running: [], queued: [], recent: [] });
  return null;
}
