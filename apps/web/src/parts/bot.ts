import type { BotSettings, ConnectionStatus, GameId, GameSummary, Speed, TimeControl } from "../api/types";
import { speedName, timeOfDay } from "../format";

/** Lichess's decline keys in plain words. */
const DECLINES: Record<string, string> = {
  generic: "declined",
  later: "busy right now",
  tooFast: "too fast",
  tooSlow: "too slow",
  timeControl: "clock not accepted",
  rated: "casual games only",
  casual: "rated games only",
  variant: "another variant",
  noBot: "bots not accepted",
  onlyBot: "bots only",
};

export const declineWords = (reason: string | null | undefined): string => (reason ? (DECLINES[reason] ?? reason) : "");

/** `Bullet, Blitz and Rapid`. */
function list(words: string[]): string {
  if (words.length <= 1) return words.join("");
  return `${words.slice(0, -1).join(", ")} and ${words.at(-1)}`;
}

/** Seconds as the shortest honest clock length: `30 s`, `1 min`, `30 min`. */
function span(seconds: number): string {
  if (seconds < 60) return `${seconds} s`;
  const m = seconds / 60;
  return `${Number.isInteger(m) ? m : m.toFixed(1)} min`;
}

/** One row of the rules: what it is and what is set. `tone` marks a setting that needs attention. */
export interface Rule {
  label: string;
  value: string;
  tone?: "warn";
}

/**
 * What the bot takes, as rows: the version that plays, the speeds, the challenges it accepts, the clocks and how many
 * games at once. A bot with no version is said to have none, never to play the champion.
 */
export function rulesItems(s: BotSettings): Rule[] {
  const kinds =
    s.accept_rated && s.accept_casual
      ? "Rated and casual"
      : s.accept_rated
        ? "Rated"
        : s.accept_casual
          ? "Casual"
          : "No";
  const who =
    s.accept_people && s.accept_bots
      ? "people and bots"
      : s.accept_people
        ? "people only"
        : s.accept_bots
          ? "bots only"
          : "nobody";
  const added = s.max_increment === 0 ? "no added time" : `up to ${span(s.max_increment)} added a move`;
  const plays: Rule = !s.version
    ? { label: "Plays", value: "No version chosen", tone: "warn" }
    : { label: "Plays", value: s.version === "champion" ? "The champion, as of each game" : s.version };
  return [
    plays,
    { label: "Speeds", value: list(s.speeds.map((x: Speed) => speedName[x])) || "None" },
    { label: "Takes", value: `${kinds} games from ${who}` },
    { label: "Clock", value: `${span(s.min_initial)} to ${span(s.max_initial)}, ${added}` },
    { label: "At once", value: String(s.max_games) },
  ];
}

// ---- What the bot is doing, and how it has done

/** Where the bot stands right now, as one line: what it is doing, or why it is not. */
export interface BotNow {
  text: string;
  /** `live` while it plays; `idle` online and free; `warn` when something needs the person; `off` when it is not playing at all. */
  tone: "live" | "idle" | "warn" | "off";
  /** The game it is playing, to watch it on the board. */
  game: GameId | null;
  /** More games than the one linked. */
  more: number;
}

/**
 * Says what the bot is doing: the game it is in, the daily limit it reached, the challenge it is waiting on, whether it
 * is looking for games, or that it is waiting for challenges. `opponent` names the other side of a game by id.
 */
export function botNow(c: ConnectionStatus, opponent: (id: GameId) => string | null): BotNow {
  const bot = c.bot;
  const none = { game: null, more: 0 };
  if (!c.configured) return { text: "No token yet", tone: "off", ...none };
  if (!bot?.online) {
    if (!bot?.settings.version) return { text: "Choose the version it plays first", tone: "warn", ...none };
    if (c.state === "error") return { text: c.error ?? "Cannot reach Lichess", tone: "warn", ...none };
    return { text: "Offline", tone: "off", ...none };
  }
  if (c.state === "connecting") return { text: "Connecting…", tone: "idle", ...none };
  const [game, ...rest] = c.playing;
  if (game !== undefined) {
    const them = opponent(game);
    return { text: them ? `Playing ${them}` : "Playing a game", tone: "live", game, more: rest.length };
  }
  if (bot.bot_limit_until) {
    return {
      text: `Daily bot-game limit reached, resets ${timeOfDay(bot.bot_limit_until)}. It still takes people's challenges`,
      tone: "warn",
      ...none,
    };
  }
  const seek = bot.seek;
  if (seek.mode !== "off") {
    if (seek.pending) {
      const p = seek.pending.opponent;
      return { text: `Seeking: challenged ${p.name} (${p.rating})`, tone: "idle", ...none };
    }
    return { text: `Seeking · ${seek.note ?? "looking for a bot to challenge"}`, tone: "idle", ...none };
  }
  return { text: "Waiting for challenges · seeking off", tone: "idle", ...none };
}

/** One finished game of the bot's, from its side. */
export interface BotGame {
  id: GameId;
  result: "W" | "L" | "D";
  opponent: string;
  /** Their rating in that game. */
  rating: number | null;
  /** The bot's own rating going into it. */
  own: number | null;
  speed: Speed;
  rated: boolean;
  clock: TimeControl;
  /** When it started, as an ISO time. */
  at: string;
}

/** The bot's side of a game it played on Lichess, or nothing for any other game. */
export function botGameOf(g: GameSummary): BotGame | null {
  if (g.source.type !== "lichess" || g.source.connection !== "bot") return null;
  if (g.status !== "finished" || !g.outcome) return null;
  const white = g.white.player === "dameless";
  const us = white ? g.white : g.black;
  const them = white ? g.black : g.white;
  const result = g.outcome === "draw" ? "D" : (g.outcome === "white_wins") === white ? "W" : "L";
  return {
    id: g.id,
    result,
    opponent: them.name,
    rating: them.rating ?? null,
    own: us.rating ?? null,
    speed: g.source.speed,
    rated: g.source.rated,
    clock: g.time,
    at: g.started_at,
  };
}

/** The bot's finished games, newest first. */
export function botResults(games: readonly GameSummary[], limit = 20): BotGame[] {
  return games
    .map(botGameOf)
    .filter((g): g is BotGame => g !== null)
    .sort((a, b) => b.at.localeCompare(a.at))
    .slice(0, limit);
}

/** Wins, draws and losses of a list of results. */
export function botTally(results: readonly BotGame[]): { w: number; d: number; l: number } {
  return {
    w: results.filter((r) => r.result === "W").length,
    d: results.filter((r) => r.result === "D").length,
    l: results.filter((r) => r.result === "L").length,
  };
}

/**
 * The bot's rating at one speed through its games, oldest first, ending at `current` (what Lichess says now). Each game
 * gives the rating it went in with. Fewer than two points is no trend.
 */
export function botTrend(
  results: readonly BotGame[],
  speed: Speed,
  current: number | null,
): { x: number[]; rating: number[]; change: number | null } {
  const rated = results.filter((r) => r.speed === speed && r.own != null).toReversed();
  const x = rated.map((r) => Math.floor(new Date(r.at).getTime() / 1000));
  const rating = rated.map((r) => r.own!);
  if (current != null && rated.length) {
    x.push(Math.floor(Date.now() / 1000));
    rating.push(current);
  }
  return { x, rating, change: rating.length >= 2 ? rating.at(-1)! - rating[0]! : null };
}
