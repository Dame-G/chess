import type { Idea, IdeaKind, IdeaStatus, Preset, Stage, Trial, TrialStatus, TrialSummary } from "../api/types";
import { plusMinus, trialClock } from "../format";

export type Tone = "neutral" | "info" | "success" | "warning" | "danger";

/** An idea's verdict as the stamp on it says it. */
export const STATUS_WORD: Record<IdeaStatus, string> = {
  kept: "Kept",
  dropped: "Dropped",
  running: "Running",
  queued: "Queued",
  stopped: "Stopped",
  failed: "Failed",
};

export const STATUS_TONE: Record<IdeaStatus, Tone> = {
  queued: "neutral",
  running: "info",
  kept: "success",
  dropped: "neutral",
  stopped: "neutral",
  failed: "danger",
};

export const TRIAL_TONE: Record<TrialStatus, Tone> = {
  waiting: "neutral",
  running: "info",
  passed: "success",
  failed: "danger",
  stopped: "neutral",
  skipped: "neutral",
};

export const KIND_WORD: Record<IdeaKind, string> = {
  gain: "Gain",
  neutral: "Neutral",
  speed: "Speed",
  tune: "Tune",
  net: "Net",
};

/** The stages as people read them. */
export const STAGE_WORD: Record<Stage, string> = {
  fingerprint: "Bench",
  screen: "Quick screen",
  short: "Short games",
  long: "Long games",
  speed: "Speed",
  tune: "Tune",
  confirm: "Confirmation",
  custom: "Test",
  train: "Training",
};

/** The clock each preset plays, as written (before it is scaled to this machine). */
const PRESET_CLOCK: Record<Preset, string> = {
  vstc: "4+0.04",
  stc: "8+0.08",
  ltc: "40+0.4",
  stc_hash: "8+0.08, 2 MB hash",
  ltc_hash: "40+0.4, 8 MB hash",
  smp: "40+0.4, 6 threads",
  nodes: "25k nodes",
};

/** A trial's clock as written: the full trial's when it was fetched, else its preset's; "" when neither says. */
export function clockOf(t: Pick<TrialSummary, "preset">, full?: Pick<Trial, "time">): string {
  if (full?.time) return trialClock(full.time);
  return t.preset ? PRESET_CLOCK[t.preset] : "";
}

/** The trial that decided an idea: the last one with an Elo estimate, else the last that ended either way. */
export function decidingTrial(idea: Pick<Idea, "trials">): TrialSummary | undefined {
  const trials = idea.trials;
  return (
    [...trials].reverse().find((t) => t.elo && t.status !== "waiting" && t.status !== "skipped") ??
    [...trials].reverse().find((t) => t.status === "passed" || t.status === "failed")
  );
}

/** `+3.1 ± 2.0` for the trial that decided an idea, or nothing when no games decided it. */
export function ideaElo(idea: Pick<Idea, "trials">): string {
  const t = decidingTrial(idea);
  return t?.elo ? plusMinus(t.elo) : "";
}

/** Where a running or queued idea is, in a few words: "Short games" or "Waiting its turn". */
export function progressWord(idea: Pick<Idea, "status" | "trials">): string {
  if (idea.status === "queued") return "Waiting its turn";
  const live = idea.trials.find((t) => t.status === "running");
  return live ? STAGE_WORD[live.stage] : "Starting";
}

/** A funnel's pairs played, from a trial's pentanomial counts. */
export const pairsOf = (penta: readonly number[]): number => penta.reduce((a, b) => a + b, 0);

/** Where in its bounds a test's evidence stands, 0 at the lower line, 1 at the upper one. */
export function gaugePosition(llr: { value: number; lower: number; upper: number }): number {
  const span = llr.upper - llr.lower;
  return span > 0 ? Math.min(1, Math.max(0, (llr.value - llr.lower) / span)) : 0.5;
}
