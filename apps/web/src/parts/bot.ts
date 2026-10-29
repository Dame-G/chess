import type { BotSettings, Speed } from "../api/types";
import { speedName } from "../format";

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

/** Seconds as the shortest honest clock length: `30 s`, `1 minute`, `30 minutes`. */
function span(seconds: number): string {
  if (seconds < 60) return `${seconds} s`;
  const m = seconds / 60;
  return `${Number.isInteger(m) ? m : m.toFixed(1)} ${m === 1 ? "minute" : "minutes"}`;
}

/**
 * What the bot takes, in a sentence: "Rated and casual Bullet, Blitz and Rapid games, from 1 to 30 minutes with up to
 * 30 s added a move, from people and bots, one at a time. Plays the champion."
 */
export function rulesSummary(s: BotSettings): string {
  const kinds =
    s.accept_rated && s.accept_casual
      ? "Rated and casual"
      : s.accept_rated
        ? "Rated"
        : s.accept_casual
          ? "Casual"
          : "No";
  const speeds = list(s.speeds.map((x: Speed) => speedName[x]));
  const who =
    s.accept_people && s.accept_bots
      ? "people and bots"
      : s.accept_people
        ? "people only"
        : s.accept_bots
          ? "bots only"
          : "nobody";
  const added = s.max_increment === 0 ? "no added time" : `up to ${span(s.max_increment)} added a move`;
  const at = s.max_games === 1 ? "one at a time" : `${s.max_games} at a time`;
  const plays = !s.version
    ? "No version is chosen."
    : `Plays ${s.version === "champion" ? "the champion" : s.version}.`;
  return `${kinds} ${speeds || "no"} games, from ${span(s.min_initial)} to ${span(s.max_initial)} with ${added}, from ${who}, ${at}. ${plays}`;
}
