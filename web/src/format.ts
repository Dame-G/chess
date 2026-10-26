import type { Experiment, Test } from "./api/queries";

type Tone = "neutral" | "info" | "success" | "warning" | "danger";

/** Stamp tones for job states. */
export const jobTone = {
  queued: "neutral",
  running: "info",
  done: "success",
  failed: "danger",
  cancelled: "neutral",
} as const satisfies Record<string, Tone>;

/** `+1.23`, `−0.50` or `0.00`: a figure that rounds to zero has no sign. */
export function signed(n: number, digits = 2): string {
  const text = Math.abs(n).toFixed(digits);
  const zero = Number(text) === 0;
  return `${zero ? "" : n > 0 ? "+" : "−"}${text}`;
}

/** A time control for people: `8+0.08` or `25k nodes`. */
export function timeControl(tc: { clock?: { seconds: number; increment: number }; nodes?: { nodes: number } }): string {
  if (tc.clock) return `${tc.clock.seconds}+${tc.clock.increment}`;
  if (tc.nodes) return `${tc.nodes.nodes / 1000}k nodes`;
  return "";
}

/** Search speed for people: `1.52M nodes/s`. */
export function speed(nps: number): string {
  const k = Math.round(nps / 1000);
  return k >= 1000 ? `${(nps / 1e6).toFixed(2)}M nodes/s` : `${k}k nodes/s`;
}

const relative = new Intl.RelativeTimeFormat("en", { numeric: "auto" });
const units: [Intl.RelativeTimeFormatUnit, number][] = [
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
export function duration(minutes: number): string {
  if (minutes < 1) return "under a minute";
  const whole = Math.round(minutes);
  if (whole < 60) return `${whole} min`;
  const h = Math.floor(whole / 60);
  const m = whole % 60;
  return m ? `${h} h ${m} min` : `${h} h`;
}

/** The stages of a funnel, in words. */
export const stages: Record<string, { label: string; detail: string; tc?: string }> = {
  bench: {
    label: "Fingerprint",
    detail:
      "Both versions search a fixed set of positions. A change to play must change the node count; a pure clean-up must not.",
  },
  speed: { label: "Speed", detail: "Both versions are timed on the same positions, in turns, to catch a slowdown." },
  screen: {
    label: "Quick screen",
    tc: "4+0.04",
    detail: "Throws out clearly bad ideas in minutes.",
  },
  stc: { label: "Short games", tc: "8+0.08", detail: "The main test, as Reckless runs it." },
  ltc: {
    label: "Long games",
    tc: "40+0.4",
    detail: "Confirms that a gain holds with more thinking time.",
  },
  match: { label: "Match", detail: "A match between the two versions." },
};

export function stageLabel(name: string): string {
  return stages[name]?.label ?? name;
}

/** A stage with its time control, the one name used in tabs, dialogs and lists: "Quick screen (4+0.04)". */
export function stageTitle(name: string): string {
  const s = stages[name];
  return s?.tc ? `${s.label} (${s.tc})` : stageLabel(name);
}

export const kinds = {
  gainer: {
    label: "Make it stronger",
    detail: "A change meant to win more games. It has to prove it is stronger.",
  },
  nonregression: {
    label: "Keep it as strong",
    detail: "A simplification or clean-up that changes play. It only has to prove it is not weaker.",
  },
  nonfunctional: {
    label: "No change to play",
    detail: "Refactors, comments, speed-ups that search exactly the same. Checked by fingerprint and speed, no games.",
  },
} as const;

/** What an experiment's state means, in a word and a sentence. */
export function verdict(e: Experiment): { label: string; tone: Tone; sentence: string } {
  if (e.status === "queued")
    return { label: "Queued", tone: "neutral", sentence: "Waiting for the runner to start it." };
  if (e.status === "cancelled") return { label: "Stopped", tone: "neutral", sentence: "Stopped before it finished." };
  if (e.status === "running") {
    const stage = e.current ? stageLabel(e.current) : "Building";
    return { label: "Running", tone: "info", sentence: `${stage} in progress.` };
  }
  if (e.status === "passed") {
    return { label: "Passed", tone: "success", sentence: e.summary ?? "" };
  }
  return { label: "Rejected", tone: "danger", sentence: e.summary ?? "" };
}

/** The SPRT's decision lines: reject below, accept above. */
export function bounds(test: Pick<Test, "spec">): { reject: number; accept: number } | undefined {
  const s = test.spec.sprt;
  if (!s) return undefined;
  return { reject: Math.log(s.beta / (1 - s.alpha)), accept: Math.log((1 - s.beta) / s.alpha) };
}

export interface Forecast {
  /** Which way the evidence leans. */
  toward: "accept" | "reject";
  /** How far from 0 to that decision line, 0 to 1. */
  progress: number;
  /** Minutes until the decision at the current pace, if the trend holds. */
  minutes?: number;
  /** Games a minute so far. */
  rate?: number;
}

/**
 * Where a running SPRT is heading. The log-likelihood ratio drifts roughly in proportion to the games played, so the
 * games still needed to reach the nearer-trending line are estimated from the pace so far.
 */
export function forecast(test: Test, now = Date.now()): Forecast | undefined {
  const b = bounds(test);
  if (!b || test.llr == null) return undefined;
  const llr = test.llr;
  const toward = llr >= 0 ? "accept" : "reject";
  const line = toward === "accept" ? b.accept : b.reject;
  const progress = Math.min(1, Math.max(0, llr / line));
  const elapsed = (now - new Date(test.created_at).getTime()) / 60_000;
  const rate = elapsed > 0 ? test.games / elapsed : undefined;
  if (!rate || progress < 0.05 || progress >= 1) return { toward, progress, rate };
  const remaining = test.games * (1 / progress - 1);
  return { toward, progress, rate, minutes: remaining / rate };
}

/** A pawn advantage as the chance to win, from White's side (Lichess's model). */
export function winChance(cp: number): number {
  return 1 / (1 + Math.exp(-0.00368208 * cp));
}

/** How a game ended, from fastchess's terms: adjudicated games were called early once the result was clear. */
export function ending(termination: string | undefined): string {
  switch (termination) {
    case "adjudication":
      return "Called early";
    case "normal":
      return "Played out";
    case "time forfeit":
      return "Lost on time";
    case "illegal move":
      return "Illegal move";
    case "stalled connection":
    case "abandoned":
      return "Engine stopped";
    default:
      return termination ? termination[0]!.toUpperCase() + termination.slice(1) : "Unknown";
  }
}

/** Why a job failed, in a sentence, without git's command line or a file path. */
export function failure(error: string | null | undefined, kind?: string): string {
  if (!error) return kind === "build" ? "The build failed." : "It failed.";
  const missing = /^no commit named (.+?) in /.exec(error);
  if (missing) return `There is no branch, tag or commit called ${missing[1]}.`;
  const first = (error.split("\n")[0] ?? "").replace(/\s*:?\s*git -C .*$/, "").trim();
  return first && first.length <= 120 && !PATH.test(first) ? first : "It failed. The runner's log has the details.";
}

/** A file path on Windows (`C:\…`) or Unix (`/home/…`, `~/…`). A branch such as `test/x` is not one. */
const PATH = /[A-Za-z]:[\\/]|(?:^|[\s'"(`])~?\/[\w.-]+\//;
