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
