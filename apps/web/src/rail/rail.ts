import type { GameSummary } from "../api/types";
import { byRailOrder } from "../stores/games";
import { isLive } from "../board/modes";

/** What the rail shows: every game you can open, yours, or the ones from Lichess. */
export type RailFilter = "all" | "you" | "lichess";

export const FILTERS: { value: RailFilter; label: string }[] = [
  { value: "all", label: "All" },
  { value: "you", label: "You" },
  { value: "lichess", label: "Lichess" },
];

/** The filter a remembered value names; one the rail no longer has reads as All. */
export function readFilter(value: string): RailFilter {
  return FILTERS.some((f) => f.value === value) ? (value as RailFilter) : "all";
}

/** The sources a person calls games: the ones played here or on Lichess, and the ones imported. */
const GAME_SOURCES = "local,lichess,import";

/** What the server is asked for a filter (`GET /api/games`): the same rule `matches` applies to a game already held. */
export function queryOf(filter: RailFilter): { player?: string; source?: string } {
  switch (filter) {
    case "all":
      return { source: GAME_SOURCES };
    case "you":
      return { player: "you" };
    case "lichess":
      return { source: "lichess,import" };
  }
}

/** Whether a game held in the app belongs under a filter, by the rule `queryOf` gives the server. */
export function matches(g: GameSummary, filter: RailFilter): boolean {
  switch (filter) {
    case "all":
      return GAME_SOURCES.split(",").includes(g.source.type);
    case "you":
      return g.you != null;
    case "lichess":
      return g.source.type === "lichess" || g.source.type === "import";
  }
}

/**
 * The games a filter shows from what the app holds: those that match and are newer than `floor` (the oldest game the
 * pages loaded so far reach; `null` once the server has no more). Live games are always in, and come first.
 */
export function railRows(games: readonly GameSummary[], filter: RailFilter, floor: string | null): GameSummary[] {
  return [...games]
    .sort(byRailOrder)
    .filter((g) => matches(g, filter) && (floor === null || isLive(g) || g.started_at >= floor));
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

/** What an empty rail says for each filter: the title, and one sentence that teaches the next step. */
export const emptyNote: Record<RailFilter, { title: string; body?: string }> = {
  all: { title: "No games yet", body: "Play an engine, or link your Lichess account to bring in your games." },
  you: { title: "You haven't played here yet", body: "Games you play here or on Lichess show up here." },
  lichess: { title: "No Lichess games", body: "Games from your account and players you imported show up here." },
};
