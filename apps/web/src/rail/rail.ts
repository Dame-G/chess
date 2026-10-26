import type { GameSummary, Stage, TrialId } from "../api/types";
import { byRailOrder } from "../stores/games";
import { isLive } from "../board/modes";

export type RailFilter = "all" | "you" | "dameless" | "lichess" | "tests";

export const FILTERS: { value: RailFilter; label: string }[] = [
  { value: "all", label: "All" },
  { value: "you", label: "You" },
  { value: "dameless", label: "Dameless" },
  { value: "lichess", label: "Lichess" },
  { value: "tests", label: "Tests" },
];

const hasDameless = (g: GameSummary) => g.white.player === "dameless" || g.black.player === "dameless";

/** Whether a game belongs under a filter. Tests are trial games; Dameless is its other games (here and on Lichess). */
function matches(g: GameSummary, filter: RailFilter): boolean {
  switch (filter) {
    case "all":
      return true;
    case "you":
      return g.you !== null;
    case "dameless":
      return hasDameless(g) && g.source.type !== "trial";
    case "lichess":
      return g.source.type === "lichess" || g.source.type === "import";
    case "tests":
      return g.source.type === "trial";
  }
}

/** A row of the rail: one game, or a trial's games folded into one. */
export type RailRow =
  | { type: "game"; key: string; game: GameSummary }
  | { type: "trial"; key: string; trial: TrialId; games: GameSummary[]; live: boolean; newest: string };

/**
 * The rail's rows for a filter: live games pinned on top, then newest first, with each trial's games folded into one
 * row placed by its newest game.
 */
export function railRows(games: readonly GameSummary[], filter: RailFilter): RailRow[] {
  const rows: RailRow[] = [];
  const trials = new Map<TrialId, Extract<RailRow, { type: "trial" }>>();
  for (const g of [...games].sort(byRailOrder)) {
    if (!matches(g, filter)) continue;
    if (g.source.type === "trial") {
      const id = g.source.trial;
      let row = trials.get(id);
      if (!row) {
        row = { type: "trial", key: `trial-${id}`, trial: id, games: [], live: false, newest: g.started_at };
        trials.set(id, row);
        rows.push(row);
      }
      row.games.push(g);
      row.live ||= isLive(g);
      continue;
    }
    rows.push({ type: "game", key: `game-${g.id}`, game: g });
  }
  const live = (r: RailRow) => (r.type === "game" ? isLive(r.game) : r.live);
  const when = (r: RailRow) => (r.type === "game" ? r.game.started_at : r.newest);
  return rows.sort((a, b) => Number(live(b)) - Number(live(a)) || when(b).localeCompare(when(a)));
}

export const stageLabel: Record<Stage, string> = {
  fingerprint: "fingerprint",
  screen: "quick screen",
  short: "short games",
  long: "long games",
  speed: "speed",
  tune: "tune",
  confirm: "confirmation",
  custom: "test",
};
