import type { Level, LevelRating, LevelsOverview, PlannedMatch } from "../api/types";
import { aboutDuration, formatNumber } from "../format";

/** Whether a level has played enough to have a rating of its own: a fit exists and games went into it. */
export const isCalibrated = (l: LevelRating): boolean => l.games > 0 && !!l.rating;

/** A level's name: `Level 5`, and `Full strength` for the top of the ladder (it plays on the clock). */
export const levelName = (level: Level): string => (level.nodes == null ? "Full strength" : `Level ${level.id}`);

/** The rating the app shows for a level: the calibrated Elo, else the nominal one (marked as an estimate by callers). */
function levelElo(l: LevelRating): number {
  return isCalibrated(l) ? Math.round(l.rating!.elo) : l.level.nominal;
}

/** `1642 ± 48` for a calibrated level, `about 1600` for one that has not played yet. */
export function levelRatingText(l: LevelRating): string {
  if (!isCalibrated(l)) return `about ${l.level.nominal}`;
  const e = l.rating!;
  return `${Math.round(e.elo)} ± ${Math.round((e.high - e.low) / 2)}`;
}

/**
 * The options of the Strength select: full strength first, then each level of the ladder with its rating, the one
 * nearest yours marked. Values are `full` or the level's id.
 */
export function strengthOptions(o: LevelsOverview | null): { value: string; label: string }[] {
  return [
    { value: "full", label: "Full strength" },
    ...(o?.levels ?? [])
      .filter((l) => l.level.nodes != null)
      .map((l) => ({
        value: String(l.level.id),
        label: `Level ${l.level.id} · ${isCalibrated(l) ? levelElo(l) : `about ${l.level.nominal}`}${l.level.id === o?.suggested ? " (suggested)" : ""}`,
      })),
  ];
}

/** The strength to play at by default when practising a position: the level nearest your rating, else full strength. */
export function suggestedStrength(o: LevelsOverview | null): string {
  const id = o?.suggested;
  if (id == null) return "full";
  const l = o?.levels.find((x) => x.level.id === id);
  return !l || l.level.nodes == null ? "full" : String(id);
}

/** The Dameless seat for "Play this out": the level nearest your rating, as the seat string the server reads. */
export function playOutSeat(o: LevelsOverview | null): string {
  const s = suggestedStrength(o);
  return s === "full" ? "dameless" : `dameless@${s}`;
}

/**
 * The level a finished game's Dameless seat played at, read back from the rating the server wrote on the seat (it
 * names a level seat by its rating). `null` for full strength or a seat that matches no level.
 */
export function levelOfRating(o: LevelsOverview | null, rating: number | null | undefined): number | null {
  if (rating == null) return null;
  const hit = (o?.levels ?? []).find((l) => l.level.nodes != null && levelElo(l) === rating);
  return hit?.level.id ?? null;
}

/** Seconds a game of a calibration match takes, by the nodes it plays: about sixty plies, measured on this machine. */
function calibrationGameSeconds(nodes: number): number {
  return 60 * (nodes / 1_000_000 + 0.01);
}

/** How long the planned calibration takes in the background, as "About 1 min": empty when nothing is planned. */
export function calibrationTime(plan: readonly PlannedMatch[], levels: readonly LevelRating[], slots: number): string {
  if (plan.length === 0) return "";
  const nodes = (id: number) => levels.find((l) => l.level.id === id)?.level.nodes ?? 1_000_000;
  const seconds = plan.reduce((n, m) => n + m.games * calibrationGameSeconds(Math.max(nodes(m.a), nodes(m.b))), 0);
  const when = aboutDuration(seconds / Math.max(1, slots));
  return when.charAt(0).toUpperCase() + when.slice(1);
}

/** What queueing calibration costs, in words: the pairs of levels, their games and the time they take in the background. */
export function calibrationCost(plan: readonly PlannedMatch[], levels: readonly LevelRating[], slots: number): string {
  if (plan.length === 0) return "";
  const games = plan.reduce((n, m) => n + m.games, 0);
  const pairs = plan.length === 1 ? "1 pair" : `${plan.length} pairs`;
  return `${games} games between ${pairs} of neighbouring levels. ${calibrationTime(plan, levels, slots)} in the background, behind everything else.`;
}

/** One rung of the ladder: a level or the marker for You, ordered by rating. */
export interface Rung {
  id: string;
  kind: "level" | "you";
  name: string;
  elo: number;
  /** A calibrated rating with its margin, or the estimate the level was designed for. */
  text: string;
  /** Whether the rating was found by playing (an estimate is drawn quieter). */
  found: boolean;
  games: number;
  /** The level nearest your rating. */
  near: boolean;
}

/**
 * The ladder as rows from weakest to strongest, with You between the two levels your rating falls under and over.
 * Levels without a rating of their own sit at the one they were designed for, marked as estimates.
 */
export function ladder(o: LevelsOverview): Rung[] {
  const rungs: Rung[] = o.levels.map((l) => ({
    id: `level-${l.level.id}`,
    kind: "level",
    name: levelName(l.level),
    elo: levelElo(l),
    text: isCalibrated(l)
      ? `${formatNumber(Math.round(l.rating!.elo))} ± ${Math.round((l.rating!.high - l.rating!.low) / 2)}`
      : `≈ ${formatNumber(l.level.nominal)}`,
    found: isCalibrated(l),
    games: l.games,
    near: l.level.id === o.suggested,
  }));
  rungs.sort((a, b) => a.elo - b.elo);
  if (o.you) {
    const e = o.you.estimate;
    const at = rungs.findIndex((r) => r.elo > e.elo);
    const you: Rung = {
      id: "you",
      kind: "you",
      name: "You",
      elo: Math.round(e.elo),
      text: `${formatNumber(Math.round(e.elo))} ± ${Math.round((e.high - e.low) / 2)}`,
      found: true,
      games: o.you.games,
      near: false,
    };
    rungs.splice(at < 0 ? rungs.length : at, 0, you);
  }
  return rungs;
}
