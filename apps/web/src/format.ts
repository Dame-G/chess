import type { Estimate, GameSummary, Outcome, Score, Seat, Side, Speed, TimeControl } from "./api/types";

/** `+1.23`, `−0.50` or `0.00`: a figure that rounds to zero has no sign. */
export function signed(n: number, digits = 2): string {
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

/** A trial's clock: `8+0.08`, where the base is in seconds as OpenBench writes it. */
export function trialClock(tc: TimeControl): string {
  if (tc.type !== "clock") return timeControl(tc);
  return `${tc.initial_ms / 1000}+${tc.increment_ms / 1000}`;
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

/** "5 minutes ago", "yesterday". */
export function ago(iso: string, now = Date.now()): string {
  const seconds = (new Date(iso).getTime() - now) / 1000;
  for (const [unit, size] of units) {
    if (Math.abs(seconds) >= size) return relative.format(Math.round(seconds / size), unit);
  }
  return "just now";
}

/** "3 min", "1 h 20 min". */
export function duration(seconds: number): string {
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

/** A side's score in a finished game: 1, ½ or 0. */
export function sideResult(outcome: Outcome | null | undefined, side: Side): "1" | "½" | "0" | null {
  if (!outcome) return null;
  if (outcome === "draw") return "½";
  return (outcome === "white_wins") === (side === "white") ? "1" : "0";
}

/** How You did in a game: won, drew or lost; `null` when You did not play or it is not over. */
export function yourResult(g: GameSummary): "won" | "drew" | "lost" | null {
  if (!g.you || !g.outcome) return null;
  const r = sideResult(g.outcome, g.you);
  return r === "1" ? "won" : r === "½" ? "drew" : "lost";
}

/** A seat as people say it: `Dameless v14`, `cutemouse83`, `Stockfish level 5`. */
export function seatName(seat: Seat, versionName?: string): string {
  if (seat.player === "dameless") return versionName ? `Dameless ${versionName}` : seat.name || "Dameless";
  return seat.name;
}

export const other = (side: Side): Side => (side === "white" ? "black" : "white");

/** An estimate as `+3.1 ± 2.0`: the figure and half its 95 % interval. */
export function plusMinus(e: Estimate, digits = 1): string {
  return `${signed(e.elo, digits)} ± ${((e.high - e.low) / 2).toFixed(digits)}`;
}

/** A figure with `digits` decimals, or an em dash when there is none. */
export function figure(n: number | null | undefined, digits = 1): string {
  return n == null ? "—" : n.toFixed(digits);
}

/** A percentage as `81.3`, or an em dash. */
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

/** `12 Oct`, with the year when it is not this one. */
export function shortDate(iso: string, now = new Date()): string {
  const d = new Date(iso);
  const sameYear = d.getFullYear() === now.getFullYear();
  return d.toLocaleDateString("en-GB", { day: "numeric", month: "short", ...(sameYear ? {} : { year: "numeric" }) });
}

/** Seconds a review takes per game on this machine, as the design quotes it. */
const REVIEW_SECONDS_PER_GAME = 20;

/** What reviewing `games` games costs, in words: "about 40 s", "about 7 min". */
export function reviewCost(games: number): string {
  const seconds = games * REVIEW_SECONDS_PER_GAME;
  return seconds < 90 ? `about ${Math.max(5, Math.round(seconds / 5) * 5)} s` : `about ${duration(seconds)}`;
}

/** `1 game`, `3 games`. */
export const plural = (n: number, one: string, many = `${one}s`): string =>
  `${n.toLocaleString("en-GB")} ${n === 1 ? one : many}`;
