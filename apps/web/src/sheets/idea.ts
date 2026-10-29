import type {
  Estimate,
  Idea,
  IdeaKind,
  IdeaStatus,
  Preset,
  Stage,
  Trial,
  TrialStatus,
  TrialSummary,
} from "../api/types";
import { plusMinus, trialClock } from "../format";
import { stageLabel } from "../rail/rail";

/** An idea's status as its badge says it (the badge's tone is `ideaTone` in `ui/badge.ts`). */
export const STATUS_WORD: Record<IdeaStatus, string> = {
  kept: "Kept",
  dropped: "Dropped",
  running: "Running",
  queued: "Queued",
  stopped: "Stopped",
  failed: "Failed",
};

/** A trial's state as its badge says it, written like an idea's (`trialTone`). */
export const TRIAL_WORD: Record<TrialStatus, string> = {
  waiting: "Waiting",
  running: "Running",
  passed: "Passed",
  failed: "Failed",
  stopped: "Stopped",
  skipped: "Skipped",
};

export const KIND_WORD: Record<IdeaKind, string> = {
  gain: "Gain",
  neutral: "Neutral",
  speed: "Speed",
  tune: "Tune",
  net: "Net",
};

/** The stages as people read them, capitalised to start a line: `Quick screen`, `Short games` (the rail's words). */
export const STAGE_WORD = Object.fromEntries(
  Object.entries(stageLabel).map(([stage, word]) => [stage, word.charAt(0).toUpperCase() + word.slice(1)]),
) as Record<Stage, string>;

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

const pairsWord = (n: number): string => `${n.toLocaleString("en-GB")} ${n === 1 ? "pair" : "pairs"}`;

/**
 * One line of evidence for an idea in a list: the running trial's Elo and pairs ("+0.7 ± 5.9 Elo after 412 pairs"),
 * else its pairs, else the trial's own words, else what the idea waits for. Empty when there is nothing to say yet.
 */
export function evidenceLine(idea: Pick<Idea, "status" | "trials">): string {
  const t = idea.trials.find((x) => x.status === "running") ?? decidingTrial(idea);
  if (!t) return "";
  // A queued idea's trial only says it waits, which the row already says.
  if (idea.status === "queued" && !t.elo && !t.pairs) return "";
  if (t.elo) return `${plusMinus(t.elo)} Elo${t.pairs ? ` after ${pairsWord(t.pairs)}` : ""}`;
  if (t.pairs) return pairsWord(t.pairs);
  return t.detail ?? "";
}

/** The running trial a list row draws a gauge for, when its evidence has started. */
export function gaugeTrial(idea: Pick<Idea, "trials">): TrialSummary | undefined {
  return idea.trials.find((t) => t.status === "running" && t.llr);
}

/** An estimate for a stat (`+0.8 ± 2.1`), its spaces fixed so a narrow column never breaks it across two lines. */
export const eloFigure = (e: Estimate): string => plusMinus(e).replaceAll(" ", " ");

/** A funnel's pairs played, from a trial's pentanomial counts. */
export const pairsOf = (penta: readonly number[]): number => penta.reduce((a, b) => a + b, 0);

/** Where in its bounds a test's evidence stands, 0 at the lower line, 1 at the upper one. */
export function gaugePosition(llr: { value: number; lower: number; upper: number }): number {
  const span = llr.upper - llr.lower;
  return span > 0 ? Math.min(1, Math.max(0, (llr.value - llr.lower) / span)) : 0.5;
}
