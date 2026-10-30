import type { Level, LevelRating, LevelsOverview, PlannedMatch } from "../api/types";
import { aboutDuration, formatNumber } from "../format";

/** Whether a level has played enough to have a rating of its own: a fit exists and games went into it. */
export const isCalibrated = (l: LevelRating): boolean => l.games > 0 && !!l.rating;

/**
 * How well the ladder knows a level: `found` (a rating from enough games, within a normal margin), `provisional` (a
 * rating from few games, or with a margin so wide that calibration has more to say), or `estimate` (it has not played:
 * only the rating it was designed for).
 */
export type Standing = "found" | "provisional" | "estimate";

/** A rating from fewer games than this is provisional. */
const PROVISIONAL_GAMES = 20;
/** So is one whose margin (half its 95 % interval) is wider than this: the fit has not narrowed yet. */
const PROVISIONAL_MARGIN = 450;

/** A level's margin: half its interval, 0 for a level that has none. */
const marginOf = (l: LevelRating): number => (l.rating ? Math.round((l.rating.high - l.rating.low) / 2) : 0);

export function standingOf(l: LevelRating): Standing {
  if (!isCalibrated(l)) return "estimate";
  return l.games < PROVISIONAL_GAMES || marginOf(l) > PROVISIONAL_MARGIN ? "provisional" : "found";
}

/** A level's name: `Level 5`, and `Full strength` for the top of the ladder (it plays on the clock). */
const levelName = (level: Level): string => (level.nodes == null ? "Full strength" : `Level ${level.id}`);

/** The rating the app shows for a level: the calibrated Elo, else the nominal one (marked as an estimate by callers). */
function levelElo(l: LevelRating): number {
  return isCalibrated(l) ? Math.round(l.rating!.elo) : l.level.nominal;
}

/** A level's rating as written: `1,378 ± 416`; `≈ 2,441 ± 489` while provisional; `≈ 2,800` before it has played. */
export function ratingText(l: LevelRating): string {
  const standing = standingOf(l);
  if (standing === "estimate") return `≈ ${formatNumber(l.level.nominal)}`;
  const text = `${formatNumber(levelElo(l))} ± ${formatNumber(marginOf(l))}`;
  return standing === "provisional" ? `≈ ${text}` : text;
}

/** One level as New game offers it: its name, rating, how well that is known, and whether it is about your strength. */
export interface LevelChoice {
  /** `full`, or the level's id: what the Strength setting remembers. */
  value: string;
  name: string;
  text: string;
  standing: Standing;
  /** The level nearest your rating. */
  near: boolean;
}

/** The levels from weakest to strongest, full strength last, each with its rating and the one nearest yours marked. */
export function levelChoices(o: LevelsOverview | null): LevelChoice[] {
  return (o?.levels ?? [])
    .toSorted((a, b) => a.level.id - b.level.id)
    .map((l) => ({
      value: l.level.nodes == null ? "full" : String(l.level.id),
      name: levelName(l.level),
      text: ratingText(l),
      standing: standingOf(l),
      near: l.level.id === o?.suggested,
    }));
}

/** A level next to another on the ladder, as "play again" offers it. */
export interface Neighbour {
  /** What to ask the server for: a level's id, or `full`. */
  strength: number | "full";
  name: string;
  text: string;
}

/**
 * The level one step `harder` (up) or easier (down) from a strength, with its rating; `null` at the end of the
 * ladder or when the ladder has not loaded.
 */
export function neighbour(o: LevelsOverview | null, from: number | "full", harder: boolean): Neighbour | null {
  const choices = levelChoices(o);
  const at = choices.findIndex((c) => c.value === String(from));
  const to = choices[at + (harder ? 1 : -1)];
  if (at < 0 || !to) return null;
  return { strength: to.value === "full" ? "full" : Number(to.value), name: to.name, text: to.text };
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

/** One rung of the ladder: a level or the marker for You. */
export interface Rung {
  id: string;
  kind: "level" | "you";
  name: string;
  elo: number;
  /** The 95 % interval around `elo`; none for a level that has not played. */
  low: number | null;
  high: number | null;
  /** The rating as written: `1,378 ± 416`, `≈ 2,441 ± 489` (provisional) or `≈ 2,800` (an estimate). */
  text: string;
  standing: Standing;
  games: number;
  /** The level nearest your rating. */
  near: boolean;
}

/**
 * The ladder as rows from the weakest level to the strongest, with You between the two levels your rating falls under
 * and over. The levels keep their own order whatever their ratings: a level still being calibrated may sit under the
 * one below it for a while, and shows its interval rather than jumping.
 */
export function ladder(o: LevelsOverview): Rung[] {
  const rungs: Rung[] = o.levels
    .toSorted((a, b) => a.level.id - b.level.id)
    .map((l) => ({
      id: `level-${l.level.id}`,
      kind: "level",
      name: levelName(l.level),
      elo: levelElo(l),
      low: isCalibrated(l) ? Math.round(l.rating!.low) : null,
      high: isCalibrated(l) ? Math.round(l.rating!.high) : null,
      text: ratingText(l),
      standing: standingOf(l),
      games: l.games,
      near: l.level.id === o.suggested,
    }));
  if (o.you) {
    const e = o.you.estimate;
    const at = rungs.findIndex((r) => r.elo > e.elo);
    const you: Rung = {
      id: "you",
      kind: "you",
      name: "You",
      elo: Math.round(e.elo),
      low: Math.round(e.low),
      high: Math.round(e.high),
      text: `${formatNumber(Math.round(e.elo))} ± ${formatNumber(Math.round((e.high - e.low) / 2))}`,
      standing: "found",
      games: o.you.games,
      near: false,
    };
    rungs.splice(at < 0 ? rungs.length : at, 0, you);
  }
  return rungs;
}
