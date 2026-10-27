import type { GameSummary, Stage, TrialId } from "../api/types";
import { byRailOrder } from "../stores/games";
import { isLive } from "../board/modes";

export type RailFilter = "all" | "you" | "dameless" | "lichess" | "tests";

export const FILTERS: { value: RailFilter; label: string }[] = [
  { value: "all", label: "All" },
  { value: "you", label: "You" },
  { value: "dameless", label: "Dameless" },
  { value: "lichess", label: "Lichess" },
  { value: "tests", label: "Trials" },
];

const hasDameless = (g: GameSummary) => g.white.player === "dameless" || g.black.player === "dameless";

/** Whether a game belongs under a filter. Tests are trial games; Dameless is its other games (here and on Lichess). */
function matches(g: GameSummary, filter: RailFilter): boolean {
  switch (filter) {
    case "all":
      return true;
    case "you":
      return g.you != null;
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

/** When a game started, as short as the rail's trailing column wants: "now", "5m", "3h", "2d", "6w", "1y". */
export function agoShort(iso: string, now = Date.now()): string {
  const minutes = Math.max(0, (now - new Date(iso).getTime()) / 60_000);
  if (minutes < 1) return "now";
  if (minutes < 60) return `${Math.floor(minutes)}m`;
  const hours = minutes / 60;
  if (hours < 24) return `${Math.floor(hours)}h`;
  const days = hours / 24;
  if (days < 14) return `${Math.floor(days)}d`;
  if (days < 365) return `${Math.floor(days / 7)}w`;
  return `${Math.floor(days / 365)}y`;
}

/** What an empty rail says for each filter. */
export const emptyNote: Record<RailFilter, string> = {
  all: "Games you play, Dameless's games on Lichess and trial games all land here.",
  you: "Games you play here or on Lichess land here.",
  dameless: "Dameless's games against you and on Lichess land here.",
  lichess: "Games from your Lichess account and imported players land here.",
  tests: "Games from an idea's trials land here, one row for each trial.",
};

export const stageLabel: Record<Stage, string> = {
  fingerprint: "fingerprint",
  screen: "quick screen",
  short: "short games",
  long: "long games",
  speed: "speed",
  tune: "tune",
  confirm: "confirmation",
  custom: "custom trial",
  train: "training",
};
