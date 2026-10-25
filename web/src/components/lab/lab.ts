import type { Experiment, Test } from "../../api/queries";
import { duration, forecast, signed, stageLabel } from "../../format";

type Result = Experiment["results"][number];
type StageState = "done" | "error" | "current" | "upcoming";

export interface FunnelStage {
  name: string;
  label: string;
  state: StageState;
  result?: Result;
  /** A few words on how it went, for the stepper. */
  note?: string;
}

const stageName = (s: Experiment["funnel"][number]) => (s.type === "match" ? s.name : s.type);

/** Every stage of an experiment's funnel, with what happened in it. */
export function funnelOf(e: Experiment, tests: readonly Test[]): FunnelStage[] {
  return e.funnel.map((s) => {
    const name = stageName(s);
    const result = e.results.find((r) => r.stage === name);
    const state: StageState = result ? (result.passed ? "done" : "error") : e.current === name ? "current" : "upcoming";
    const test = tests.find((t) => t.stage === name);
    let note: string | undefined;
    if (test?.estimate) note = `${signed(test.estimate.elo, 1)} Elo`;
    else if (name === "bench" && result) note = /unchanged/.test(result.detail) ? "same" : "changed";
    else if (name === "speed" && result) note = result.detail.match(/[+−-][\d.]+%/)?.[0]?.replace("-", "−");
    else if (state === "current") note = "running";
    return { name, label: stageLabel(name), state, result, note };
  });
}

/** The stage to show first: the one running, else the last one that ran. */
export function defaultStage(e: Experiment): string {
  const names = e.funnel.map(stageName);
  return e.current ?? e.results.at(-1)?.stage ?? names[0] ?? "bench";
}

/** One plain sentence on where the experiment stands. */
export function verdictLine(e: Experiment, tests: readonly Test[]): string {
  const live = tests.find((t) => t.running);
  switch (e.status) {
    case "queued":
      return "Waiting for the runner to start it.";
    case "running": {
      if (!live) return e.current ? `${stageLabel(e.current)} in progress.` : "Preparing the engines.";
      const f = forecast(live);
      if (!f || f.progress < 0.15) return "Too early to call. It keeps playing until the evidence is clear.";
      const way = f.toward === "accept" ? "passing" : "failing";
      return f.minutes !== undefined
        ? `Leaning towards ${way}, about ${duration(f.minutes)} to go.`
        : `Leaning towards ${way}.`;
    }
    case "cancelled": {
      const last = e.results.at(-1);
      return last ? `Stopped after ${stageLabel(last.stage).toLowerCase()}.` : "Stopped before it started.";
    }
    case "passed":
      return e.kind === "gainer"
        ? "Passed every stage: the new version is stronger, so the change is worth keeping."
        : e.kind === "nonregression"
          ? "Passed every stage: the new version is no weaker, so the clean-up is safe to keep."
          : "The play is identical and the speed is no worse, so the change is safe to keep.";
    default: {
      const bad = e.results.find((r) => !r.passed);
      if (!bad) return "It did not pass.";
      const where = stageLabel(bad.stage).toLowerCase();
      if (!bad.test) return `Failed the ${where} check: ${bad.detail}.`;
      const test = tests.find((t) => t.id === bad.test);
      const games = test ? ` after ${test.games.toLocaleString()} games` : "";
      if (bad.stage === "screen") return `Rejected at the ${where}${games}: the new version loses.`;
      return e.kind === "gainer"
        ? `Rejected at the ${where}${games}: it could not show the new version is stronger.`
        : `Rejected at the ${where}${games}: the new version is weaker.`;
    }
  }
}

/** The library row's one line: how it is doing, or how it ended. */
export function headline(e: Experiment): string {
  if (e.status === "queued") return "Waiting…";
  if (e.status === "running") return e.current ? `Testing… ${stageLabel(e.current).toLowerCase()}` : "Testing…";
  if (e.status === "cancelled") return "Stopped";
  // The runner's summary leads with Elo, the figure the stage shows too.
  const elo = e.summary?.match(/([+-]?\d+(?:\.\d+)?) ± (\d+(?:\.\d+)?) Elo/);
  if (elo) return `${signed(Number(elo[1]), 1)} ± ${Number(elo[2]).toFixed(1)} Elo`;
  const all = e.summary?.match(/^(\d+) of (\d+) stages$/);
  if (all) return all[1] === all[2] ? `All ${all[1]} stages passed` : (e.summary ?? "");
  return e.summary ?? "";
}

/** A game from the new version's side. */
export function outcome(g: { white: string; result: string }): "W" | "D" | "L" {
  if (g.result !== "1-0" && g.result !== "0-1") return "D";
  return (g.white === "dev") === (g.result === "1-0") ? "W" : "L";
}

/** "5m", "3h", "2d": a time for a crowded row. */
export function shortAgo(iso: string, now = Date.now()): string {
  const s = Math.max(0, (now - new Date(iso).getTime()) / 1000);
  if (s < 60) return "now";
  if (s < 3600) return `${Math.floor(s / 60)}m`;
  if (s < 86_400) return `${Math.floor(s / 3600)}h`;
  return `${Math.floor(s / 86_400)}d`;
}
