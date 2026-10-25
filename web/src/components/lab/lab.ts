import type { Experiment, Test } from "../../api/queries";
import { duration, forecast, signed, stageLabel, verdict } from "../../format";

type Result = Experiment["results"][number];
type StageState = "done" | "error" | "current" | "stopped" | "upcoming";

export interface FunnelStage {
  name: string;
  label: string;
  state: StageState;
  result?: Result;
  /** The stage's headline figure, for its tab: "-6.6", "changed", "identical", "+0.3%", or "—" before it has one. */
  figure: string;
  /** The margin of the figure, "± 5.2", shown smaller beside it. */
  spread?: string;
  /** What the figure measures: "Elo". Absent when the figure says it all. */
  unit?: string;
}

const stageName = (s: Experiment["funnel"][number]) => (s.type === "match" ? s.name : s.type);

type Headline = Pick<FunnelStage, "figure" | "spread" | "unit">;

/** One stage's result as a figure. */
function stageFigure(name: string, state: StageState, result: Result | undefined, test: Test | undefined): Headline {
  if (test?.estimate)
    return { figure: signed(test.estimate.elo, 1), spread: `± ${test.estimate.elo_error.toFixed(1)}`, unit: "Elo" };
  if (name === "bench" && result)
    return { figure: /unchanged|identical/.test(result.detail) ? "identical" : "changed" };
  if (name === "speed" && result) {
    const m = result.detail.match(/([+-]\d+(?:\.\d+)?)% ± (\d+(?:\.\d+)?)%/);
    if (m) return { figure: `${signed(Number(m[1]), 1)}%`, spread: `± ${Number(m[2]).toFixed(1)}`, unit: "speed" };
  }
  if (state === "current") return { figure: name === "bench" ? "checking" : name === "speed" ? "timing" : "starting" };
  if (state === "upcoming") return { figure: "—" };
  return { figure: result?.passed ? "passed" : "failed" };
}

/** Every stage of an experiment's funnel, with what happened in it. */
export function funnelOf(e: Experiment, tests: readonly Test[]): FunnelStage[] {
  return e.funnel.map((s) => {
    const name = stageName(s);
    const result = e.results.find((r) => r.stage === name);
    const test = tests.find((t) => t.stage === name);
    const state: StageState = result
      ? result.passed
        ? "done"
        : "error"
      : e.current === name
        ? "current"
        : test
          ? "stopped"
          : "upcoming";
    return { name, label: stageLabel(name), state, result, ...stageFigure(name, state, result, test) };
  });
}

/** The stage to show first: the one running, else the last one that played games or ran. */
export function defaultStage(e: Experiment, tests: readonly Test[] = []): string {
  const names = e.funnel.map(stageName);
  const played = names.filter((n) => tests.some((t) => t.stage === n)).at(-1);
  return e.current ?? played ?? e.results.at(-1)?.stage ?? names[0] ?? "bench";
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
      const cut = e.funnel
        .map(stageName)
        .map((n) => tests.find((t) => t.stage === n && !e.results.some((r) => r.stage === n)))
        .findLast(Boolean);
      if (cut)
        return `Stopped during the ${stageLabel(cut.stage).toLowerCase()}, after ${cut.games.toLocaleString()} games.`;
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

/** The library row's line: the status in words, then the figure behind it. */
export function headline(e: Experiment): { label: string; figure: string } {
  const label = verdict(e).label;
  if (e.status === "queued" || e.status === "cancelled") return { label, figure: "" };
  if (e.status === "running") return { label, figure: e.current ? stageLabel(e.current).toLowerCase() : "" };
  // The runner's summary leads with Elo, the figure the stage shows too.
  const elo = e.summary?.match(/([+-]?\d+(?:\.\d+)?) ± (\d+(?:\.\d+)?) Elo/);
  if (elo) return { label, figure: `${signed(Number(elo[1]), 1)} ± ${Number(elo[2]).toFixed(1)} Elo` };
  const speed = e.results.find((r) => r.stage === "speed")?.detail.match(/([+-]\d+(?:\.\d+)?)% ± (\d+(?:\.\d+)?)%/);
  if (speed) return { label, figure: `speed ${signed(Number(speed[1]), 1)}%` };
  return { label, figure: "" };
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

/** Splits text around the search words, so the matches can be marked. */
export function highlight(text: string, query: string): { text: string; hit: boolean }[] {
  const q = query.trim().toLowerCase();
  const at = q ? text.toLowerCase().indexOf(q) : -1;
  if (at < 0) return [{ text, hit: false }];
  return [
    { text: text.slice(0, at), hit: false },
    { text: text.slice(at, at + q.length), hit: true },
    { text: text.slice(at + q.length), hit: false },
  ].filter((s) => s.text);
}

/** The games split by result, as shares of the total, for the W–D–L bar. */
export function record(games: readonly { white: string; result: string }[]): { W: number; D: number; L: number } {
  const r = { W: 0, D: 0, L: 0 };
  for (const g of games) r[outcome(g)]++;
  return r;
}

/** A runner's result line in plain words: the stage names the new version, and big counts get separators. */
export function plainDetail(detail: string): string {
  return detail
    .replace(/^dev speed/, "The new version's speed is")
    .replace(/^bench unchanged at/, "The fingerprint is unchanged at")
    .replace(/^bench (?=\d)/, "The fingerprint changed: ")
    .replace(/\d{4,}/g, (n) => Number(n).toLocaleString());
}
