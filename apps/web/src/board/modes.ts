import type { GameSummary } from "../api/types";

/** The panel's tabs. */
export type Mode = "play" | "watch" | "review" | "explore" | "drill";

export const modeLabel: Record<Mode, string> = {
  play: "Play",
  watch: "Watch",
  review: "Review",
  explore: "Explore",
  drill: "Drill",
};

/** Whether a game is being played now. */
export const isLive = (g: Pick<GameSummary, "status">) => g.status === "live" || g.status === "starting";

/** Whether a game is played through You's Lichess account, where nothing but the game may be on screen while it runs. */
export const onLichessAsYou = (g: Pick<GameSummary, "source">) =>
  g.source.type === "lichess" && g.source.connection === "you";

/**
 * The tabs that apply to what is on the board: a free position is Explore; a live game You sit in is Play and nothing
 * else; a live game You watch is Watch (and Explore from any of its positions); a finished game is Review and Explore.
 */
export function modesFor(game: Pick<GameSummary, "status" | "you"> | null): Mode[] {
  if (!game) return ["explore"];
  if (isLive(game)) return game.you ? ["play"] : ["watch", "explore"];
  return ["review", "explore"];
}

/**
 * What opens first at `/` with nothing chosen: a live game (yours first, then the bot's, then a trial's), else your last
 * finished game, else nothing (the start position in Explore). `games` is newest first.
 */
export function firstGame(games: readonly GameSummary[]): GameSummary | undefined {
  const live = games.filter(isLive);
  return (
    live.find((g) => g.you) ??
    live.find((g) => g.source.type === "lichess" && g.source.connection === "bot") ??
    live.find((g) => g.source.type === "trial") ??
    games.find((g) => g.you && g.status === "finished")
  );
}
