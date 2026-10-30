import type { Ending, Estimate, Outcome, Progress, Score, Seat, Side, Speed, TimeControl } from "./api/types";

/** `+1.23`, `−0.50` or `0.00`: a figure that rounds to zero has no sign. */
function signed(n: number, digits = 2): string {
  const text = Math.abs(n).toFixed(digits);
  const zero = Number(text) === 0;
  return `${zero ? "" : n > 0 ? "+" : "−"}${text}`;
}

/** A White-POV score in pawns, mates as ±100 so a bar or a graph can draw them. `mate: 0` is a mate already given. */
export function pawns(score: Score, turn: Side = "white"): number {
  if ("cp" in score) return score.cp / 100;
  if (score.mate === 0) return turn === "white" ? -100 : 100;
  return score.mate > 0 ? 100 : -100;
}

/** A White-POV score as people write it: `+0.45`, `−1.20`, `M3` (White mates), `−M2`, `#` once mated. */
export function scoreText(score: Score, turn: Side = "white"): string {
  if ("cp" in score) return signed(score.cp / 100);
  if (score.mate === 0) return turn === "white" ? "0-1" : "1-0";
  return `${score.mate < 0 ? "−" : ""}M${Math.abs(score.mate)}`;
}

/** White's chance to win from a pawn advantage (Lichess's model), 0 to 1. */
export function winChance(pawnsUp: number): number {
  return 1 / (1 + Math.exp(-0.00368208 * pawnsUp * 100));
}

/** A time control as the UI and CLI write it: `3+2`, `8+0.08`, `3 days`, `1 s a move`, `25k nodes`. */
export function timeControl(tc: TimeControl): string {
  switch (tc.type) {
    case "clock": {
      const minutes = tc.initial_ms / 60_000;
      const base = minutes >= 1 && Number.isInteger(minutes) ? String(minutes) : String(tc.initial_ms / 1000);
      return `${base}+${tc.increment_ms / 1000}`;
    }
    case "correspondence":
      return `${tc.days} ${tc.days === 1 ? "day" : "days"}`;
    case "movetime":
      return `${tc.ms / 1000} s a move`;
    case "nodes":
      return `${tc.nodes >= 1000 ? `${tc.nodes / 1000}k` : tc.nodes} nodes`;
  }
}

export const speedName: Record<Speed, string> = {
  ultra_bullet: "UltraBullet",
  bullet: "Bullet",
  blitz: "Blitz",
  rapid: "Rapid",
  classical: "Classical",
  correspondence: "Correspondence",
};

/** A clock reading: `4:59`, `0:09.4` under ten seconds, `1:02:00` over an hour. */
export function clock(ms: number): string {
  const t = Math.max(0, ms);
  const total = Math.floor(t / 1000);
  const h = Math.floor(total / 3600);
  const m = Math.floor((total % 3600) / 60);
  const s = total % 60;
  if (h) return `${h}:${String(m).padStart(2, "0")}:${String(s).padStart(2, "0")}`;
  if (t < 10_000) return `0:${String(s).padStart(2, "0")}.${Math.floor((t % 1000) / 100)}`;
  return `${m}:${String(s).padStart(2, "0")}`;
}

const relative = new Intl.RelativeTimeFormat("en", { numeric: "auto" });
const units: [Intl.RelativeTimeFormatUnit, number][] = [
  ["year", 31_536_000],
  ["month", 2_592_000],
  ["week", 604_800],
  ["day", 86_400],
  ["hour", 3_600],
  ["minute", 60],
];

/** A time to expect, `about 7 min`, or `under a minute` (never "about under a minute"). */
export const aboutDuration = (seconds: number): string =>
  seconds < 60 ? "under a minute" : `about ${duration(seconds)}`;

/** "5 minutes ago", "yesterday": `formatRelative`, under its older name. */
export const ago = (iso: string, now = Date.now()): string => formatRelative(iso, now);

/** "3 min", "1 h 20 min". */
function duration(seconds: number): string {
  const minutes = seconds / 60;
  if (minutes < 1) return "under a minute";
  const whole = Math.round(minutes);
  if (whole < 60) return `${whole} min`;
  const h = Math.floor(whole / 60);
  const m = whole % 60;
  return m ? `${h} h ${m} min` : `${h} h`;
}

/** The result as a score line: `1-0`, `0-1`, `½-½`, or `*` while it runs. */
export function resultText(outcome: Outcome | null | undefined): string {
  if (outcome === "white_wins") return "1-0";
  if (outcome === "black_wins") return "0-1";
  if (outcome === "draw") return "½-½";
  return "*";
}

/** The engine id in an engine's player key (`engine:stockfish` → `stockfish`), or `null` for anyone else. */
export const engineOf = (player: string): string | null => (player.startsWith("engine:") ? player.slice(7) : null);

/** An engine at a level of its ladder (the server names it by its rating, `Stockfish 1650`). */
export const isLevelSeat = (seat: Seat): boolean => engineOf(seat.player) !== null && seat.level != null;

/** A seat as people say it: `Stockfish 1650` (at a level), `Reckless`, `cutemouse83`. The server names every seat. */
export const seatName = (seat: Seat): string => seat.name;

export const other = (side: Side): Side => (side === "white" ? "black" : "white");

/** An estimate as `+3.1 ± 2.0`: the figure and half its 95 % interval. */
export function plusMinus(e: Estimate, digits = 1): string {
  return `${signed(e.elo, digits)} ± ${((e.high - e.low) / 2).toFixed(digits)}`;
}

/** A figure with `digits` decimals, or an en dash when there is none (see `NONE`). */
export function figure(n: number | null | undefined, digits = 1): string {
  return n == null ? NONE : n.toFixed(digits);
}

/** A percentage as `81.3`, or an en dash. */
export const percent = (n: number | null | undefined, digits = 1): string => figure(n, digits);

/** A file size as `412 MB`, `1.3 GB`. */
export function bytes(n: number): string {
  const units = ["B", "kB", "MB", "GB", "TB"];
  let v = n;
  let i = 0;
  while (v >= 1000 && i < units.length - 1) {
    v /= 1000;
    i++;
  }
  return `${i === 0 || v >= 100 ? Math.round(v) : v.toFixed(1)} ${units[i]}`;
}

const MONTHS = ["Jan", "Feb", "Mar", "Apr", "May", "Jun", "Jul", "Aug", "Sep", "Oct", "Nov", "Dec"];

/** `12 Oct`, with the year when it is not this one: `formatDate`, under its older name. The months are written here, so every browser spells them alike. */
export const shortDate = (iso: string, now = new Date()): string => formatDate(iso, now);

/** A time of day as `14:30`, in the viewer's time zone. */
export function timeOfDay(when: string | number | Date): string {
  const d = new Date(when);
  return `${String(d.getHours()).padStart(2, "0")}:${String(d.getMinutes()).padStart(2, "0")}`;
}

/** Seconds a review takes per game on this machine, as the design quotes it. */
const REVIEW_SECONDS_PER_GAME = 20;

/** What reviewing `games` games costs, in words: "about 40 s", "about 7 min", "about 21 h", "about 3 days". */
export function reviewCost(games: number): string {
  const seconds = games * REVIEW_SECONDS_PER_GAME;
  if (seconds < 90) return `about ${Math.max(5, Math.round(seconds / 5) * 5)} s`;
  const hours = Math.round(seconds / 3600);
  if (hours >= 36) return `about ${Math.round(hours / 24)} days`;
  if (hours >= 2) return `about ${hours} h`;
  return `about ${duration(seconds)}`;
}

/** How far a piece of work is, in words: its own sentence when it has one, else "120 of 3,708 games". */
export function progressText(progress: Progress | null | undefined): string | null {
  if (!progress) return null;
  if (progress.text) return progress.text;
  const done = progress.done.toLocaleString("en-GB");
  return progress.total != null
    ? `${done} of ${plural(progress.total, progress.unit.replace(/s$/, ""))}`
    : `${done} ${progress.unit}`;
}

/** The `limit` that means every game: the server takes the newest N, and no one has this many. */
export const ALL_LIMIT = 1_000_000;

/** The windows a profile can add up, newest games first; the last is every game. */
export const WINDOWS = [100, 200, 500, ALL_LIMIT];

/** A window as a choice: "Last 200 games", "All games". */
export const windowName = (limit: number): string => (limit >= ALL_LIMIT ? "All games" : `Last ${limit} games`);

/** What a profile's figures cover, as a sentence tail: "your last 200 games", "all 3,708 games". */
export function coverage(limit: number, games: number, whose = "their"): string {
  return limit >= ALL_LIMIT || games < limit
    ? `all ${games.toLocaleString("en-GB")} games`
    : `${whose} last ${limit} games`;
}

/** `1 game`, `3 games`. */
export const plural = (n: number, one: string, many = `${one}s`): string =>
  `${n.toLocaleString("en-GB")} ${n === 1 ? one : many}`;

/** How a game ended, in words: `Checkmate`, `Black resigned`, `Draw by repetition`, `White ran out of time`. */
export function endingText(ending: Ending, outcome: Outcome | null | undefined): string {
  const loser = outcome === "white_wins" ? "Black" : outcome === "black_wins" ? "White" : null;
  switch (ending) {
    case "checkmate":
      return "Checkmate";
    case "resignation":
      return loser ? `${loser} resigned` : "Resigned";
    case "timeout":
      return loser ? `${loser} ran out of time` : "Draw: time ran out and no mate was possible";
    case "stalemate":
      return "Stalemate";
    case "repetition":
      return "Draw by repetition";
    case "fifty_moves":
      return "Draw by the fifty-move rule";
    case "insufficient_material":
      return "Draw: neither side can mate";
    case "agreement":
      return "Draw by agreement";
    case "adjudication":
      return "Adjudicated";
    case "abandoned":
      return loser ? `${loser} left the game` : "Abandoned";
    case "aborted":
      return "Aborted";
    case "engine_failure":
      return "An engine failed";
    case "illegal_move":
      return loser ? `${loser} played an illegal move` : "An illegal move";
    default:
      return "Game over";
  }
}

/* ------------------------------------------------------------------------------------------------------------------
   The canonical formatters of the UI kit (docs/ui-brief.md, 3.2, 3.8, 3.10). New code calls these. Every one writes a
   missing value as the same en dash, uses a true minus sign, and groups thousands the same way, so a figure reads alike
   in a stat, a table, a tooltip and a toast.
   ------------------------------------------------------------------------------------------------------------------ */

/** What a missing figure looks like everywhere: an en dash, never "0", "?" or "-". */
export const NONE = "–";

const MINUS = "−";

/** A figure with thousands separators and `digits` decimals: `3,708`, `81.3`, `−12`. */
export function formatNumber(n: number | null | undefined, digits = 0): string {
  if (n == null || !Number.isFinite(n)) return NONE;
  const text = Math.abs(n).toLocaleString("en-GB", { minimumFractionDigits: digits, maximumFractionDigits: digits });
  return n < 0 && Number(text.replace(/,/g, "")) !== 0 ? `${MINUS}${text}` : text;
}

/** An Elo difference, always signed: `+12`, `−5`, `0`; with `digits` decimals when it is an estimate. */
export function formatElo(n: number | null | undefined, digits = 0): string {
  if (n == null || !Number.isFinite(n)) return NONE;
  const text = Math.abs(n).toLocaleString("en-GB", { minimumFractionDigits: digits, maximumFractionDigits: digits });
  return Number(text.replace(/,/g, "")) === 0 ? text : `${n < 0 ? MINUS : "+"}${text}`;
}

/** An estimate with its error, `+3.1 ± 2.0`: the figure and half its 95 % interval (`error` is already the half). */
export function formatEloPm(elo: number | null | undefined, error: number | null | undefined, digits = 1): string {
  if (elo == null || error == null) return NONE;
  return `${formatElo(elo, digits)} ± ${Math.abs(error).toFixed(digits)}`;
}

/** A percentage from 0 to 100: `81.3%`. */
export function formatPercent(n: number | null | undefined, digits = 1): string {
  if (n == null || !Number.isFinite(n)) return NONE;
  return `${formatNumber(n, digits)}%`;
}

/** A rating as it is shown beside a name: `1,840`, with `?` when it is still provisional. */
export function formatRating(rating: number | string | null | undefined, provisional = false): string {
  if (rating == null || rating === "") return NONE;
  const text = typeof rating === "number" ? formatNumber(Math.round(rating)) : rating;
  return provisional ? `${text}?` : text;
}

/** A length of time as people read it: `45 s`, `3 min`, `1 h 20 min`, `2 d 3 h`. Whole units, no decimals. */
export function formatDuration(seconds: number | null | undefined): string {
  if (seconds == null || !Number.isFinite(seconds)) return NONE;
  const s = Math.max(0, Math.round(seconds));
  if (s < 60) return `${s} s`;
  const minutes = Math.round(s / 60);
  if (minutes < 60) return `${minutes} min`;
  const hours = Math.floor(minutes / 60);
  if (hours < 24) return minutes % 60 ? `${hours} h ${minutes % 60} min` : `${hours} h`;
  const days = Math.floor(hours / 24);
  return hours % 24 ? `${days} d ${hours % 24} h` : `${days} d`;
}

/** How long ago, or how far ahead: `just now`, `5 minutes ago`, `yesterday`, `in 2 hours`. */
export function formatRelative(when: string | number | Date, now: number = Date.now()): string {
  const seconds = (new Date(when).getTime() - now) / 1000;
  for (const [unit, size] of units) {
    if (Math.abs(seconds) >= size) return relative.format(Math.round(seconds / size), unit);
  }
  return "just now";
}

/** A day as `12 Oct`, with the year when it is not this one. */
export function formatDate(when: string | number | Date, now: Date = new Date()): string {
  const d = new Date(when);
  const year = d.getFullYear() === now.getFullYear() ? "" : ` ${d.getFullYear()}`;
  return `${d.getDate()} ${MONTHS[d.getMonth()]}${year}`;
}

/** A moment in full, for a tooltip: `12 Oct 2026, 14:30`. */
export function formatDateTime(when: string | number | Date): string {
  const d = new Date(when);
  return `${d.getDate()} ${MONTHS[d.getMonth()]} ${d.getFullYear()}, ${timeOfDay(d)}`;
}

/** One decimal, a sign unless it rounds to zero: `+0.6`, `−4.2`, `0.0`. */
function signedOne(pawnsUp: number): string {
  const text = Math.abs(pawnsUp).toFixed(1);
  return Number(text) === 0 ? text : `${pawnsUp < 0 ? MINUS : "+"}${text}`;
}

/** A White-POV score as `+0.6`, `−4.2` or `#3` (mate in 3; `#−3` when Black mates). `mate: 0` is `#0`. */
export function formatScore(score: Score | number | null | undefined): string {
  if (score == null) return NONE;
  if (typeof score === "number") return signedOne(score);
  if ("cp" in score) return signedOne(score.cp / 100);
  return score.mate < 0 ? `#${MINUS}${-score.mate}` : `#${score.mate}`;
}

/** Which side a White-POV score favours: `null` when it is level. A mate of 0 is against the side to move. */
export function scoreSide(score: Score | number | null | undefined, turn: Side = "white"): Side | null {
  if (score == null) return null;
  const pawnsUp = typeof score === "number" ? score : "cp" in score ? score.cp / 100 : null;
  if (pawnsUp == null) {
    const mate = (score as { mate: number }).mate;
    return mate === 0 ? other(turn) : mate > 0 ? "white" : "black";
  }
  return Number(Math.abs(pawnsUp).toFixed(1)) === 0 ? null : pawnsUp > 0 ? "white" : "black";
}

/** A result as a game record writes it. */
export type Result = "1-0" | "0-1" | "1/2-1/2" | "*";

/** The record of a game's outcome; `null` for a game with none. */
export function resultOf(outcome: Outcome | null | undefined): Result | null {
  if (outcome === "white_wins") return "1-0";
  if (outcome === "black_wins") return "0-1";
  if (outcome === "draw") return "1/2-1/2";
  return null;
}

/** How a result reads: its text and, when you played it, whether you won, lost or drew. Others' games have no tone. */
export interface ResultView {
  text: string;
  tone: "win" | "loss" | "draw" | null;
}

/** A result as `1-0`, `0-1`, `½-½` or `*`; with `you` set it also says whether it was yours to win. */
export function formatResult(result: Result | null | undefined, you?: Side | null): ResultView {
  const text = result === "1/2-1/2" ? "½-½" : (result ?? "*");
  if (!you || !result || result === "*") return { text, tone: null };
  if (result === "1/2-1/2") return { text, tone: "draw" };
  return { text, tone: (result === "1-0") === (you === "white") ? "win" : "loss" };
}
