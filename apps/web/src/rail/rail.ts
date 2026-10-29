import type { GameSummary, Stage } from "../api/types";
import { byRailOrder } from "../stores/games";
import { isLive } from "../board/modes";

/** What the rail shows: every game that is yours or Dameless's, one group of them, or the trials (a row for each). */
export type RailFilter = "all" | "you" | "dameless" | "lichess" | "trials";

export const FILTERS: { value: RailFilter; label: string }[] = [
  { value: "all", label: "All" },
  { value: "you", label: "You" },
  { value: "dameless", label: "Dameless" },
  { value: "lichess", label: "Lichess" },
  { value: "trials", label: "Trials" },
];

/** A filter the server answers with games, as opposed to Trials, which is a row for each trial (its games come when it opens). */
export type GameFilter = Exclude<RailFilter, "trials">;

/** The filter a remembered value names: the old `tests` is `trials` now. */
export function readFilter(value: string): RailFilter {
  if (value === "tests") return "trials";
  return FILTERS.some((f) => f.value === value) ? (value as RailFilter) : "all";
}

/** The sources a person calls games: the ones they or Dameless played, here or on Lichess (trial games are the Trials'). */
const GAME_SOURCES = "local,lichess,import";

/** What the server is asked for a filter (`GET /api/games`): the same rule `matches` applies to a game already held. */
export function queryOf(filter: GameFilter): { player?: string; source?: string } {
  switch (filter) {
    case "all":
      return { source: GAME_SOURCES };
    case "you":
      return { player: "you" };
    case "dameless":
      return { player: "dameless", source: GAME_SOURCES };
    case "lichess":
      return { source: "lichess,import" };
  }
}

const hasDameless = (g: GameSummary) => g.white.player === "dameless" || g.black.player === "dameless";

/**
 * Whether a game held in the app belongs under a filter, by the rule `queryOf` gives the server. A trial's game is
 * under `all` only while it is being played (the rail folds those into the trial's one row).
 */
export function matches(g: GameSummary, filter: GameFilter): boolean {
  const trial = g.source.type === "trial";
  switch (filter) {
    case "all":
      return !trial || isLive(g);
    case "you":
      return g.you != null;
    case "dameless":
      return hasDameless(g) && !trial;
    case "lichess":
      return g.source.type === "lichess" || g.source.type === "import";
  }
}

/** A row of the rail: one game, or a running trial's games folded into one. */
export type RailRow =
  | { type: "game"; key: string; game: GameSummary }
  | { type: "trial"; key: string; trial: number; games: GameSummary[]; live: boolean; newest: string };

/**
 * The games a filter shows from what the app holds: those that match and are newer than `floor` (the oldest game the
 * pages loaded so far reach; `null` once the server has no more). Live games are always in, and come first.
 */
export function railRows(games: readonly GameSummary[], filter: GameFilter, floor: string | null): RailRow[] {
  const rows: RailRow[] = [];
  const trials = new Map<number, Extract<RailRow, { type: "trial" }>>();
  for (const g of [...games].sort(byRailOrder)) {
    if (!matches(g, filter)) continue;
    if (floor !== null && !isLive(g) && g.started_at < floor) continue;
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

/**
 * The month a game belongs under, for the separator before it: `null` for a game from the last two weeks (the short
 * times beside each row say it), else `October`, or `October 2025` from another year.
 */
export function monthOf(iso: string, now = Date.now()): string | null {
  const at = new Date(iso);
  if (now - at.getTime() < 14 * 86_400_000) return null;
  const month = at.toLocaleString("en", { month: "long" });
  return at.getFullYear() === new Date(now).getFullYear() ? month : `${month} ${at.getFullYear()}`;
}

/** What an empty rail says for each filter: the title, one sentence that teaches the next step, and what to offer. */
export const emptyNote: Record<RailFilter, { title: string; body?: string }> = {
  all: { title: "No games yet", body: "Play Dameless, or link your Lichess account to bring in your games." },
  you: { title: "You haven't played here yet", body: "Games you play here or on Lichess show up here." },
  dameless: { title: "Dameless hasn't played yet", body: "Its games against you and on Lichess show up here." },
  lichess: { title: "No Lichess games", body: "Games from your account and players you imported show up here." },
  trials: { title: "No trials yet", body: "Each trial an idea runs is one row here, with its games inside." },
};

export const stageLabel: Record<Stage, string> = {
  fingerprint: "fingerprint",
  screen: "quick screen",
  short: "short games",
  long: "long games",
  speed: "speed",
  tune: "tune",
  confirm: "confirmation",
  custom: "single trial",
  train: "training",
};
