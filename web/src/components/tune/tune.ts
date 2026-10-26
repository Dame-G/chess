import type { Param, Tune, TuneDetail } from "../../api/queries";
import { duration, signed, timeControl } from "../../format";

export type Iteration = TuneDetail["history"][number];

/** Whether a tune is still on the runner's books. */
export const live = (t: Pick<Tune, "status">) => t.status === "queued" || t.status === "running";

/** A tune that stopped before its last iteration, which can carry on. */
export const resumable = (t: Pick<Tune, "status" | "iteration" | "iterations">) =>
  (t.status === "failed" || t.status === "cancelled") && t.iteration < t.iterations;

/** The tune's progress as "240/1000". */
const progressText = (t: Pick<Tune, "iteration" | "iterations">) => `${t.iteration}/${t.iterations}`;

/** The status, in the few words a library row has room for. */
export function statusWord(t: Tune): string {
  switch (t.status) {
    case "queued":
      return "Waiting";
    case "running":
      return `Iteration ${progressText(t)}`;
    case "done":
      return "Finished";
    case "failed":
      return "Failed";
    default:
      return "Stopped";
  }
}

/** Values are integers in the engine (`i32`), so a parameter shows as one unless its declaration has decimals. */
export function fmt(p: Pick<Param, "start" | "min" | "max">, v: number): string {
  const whole = [p.start, p.min, p.max].every(Number.isInteger);
  const n = whole ? Math.round(v) : Math.round(v * 100) / 100;
  return n.toLocaleString("en", { maximumFractionDigits: 2 }).replace("-", "−");
}

/** How far a parameter has moved from its start, in its own units (rounded as applying would) and as a share of its range. */
export function change(p: Pick<Param, "start" | "min" | "max">, v: number) {
  const whole = [p.start, p.min, p.max].every(Number.isInteger);
  const delta = (whole ? Math.round(v) : v) - p.start;
  const span = p.max - p.min;
  const share = span > 0 ? (delta / span) * 100 : 0;
  return { delta, share, text: signed(whole ? delta : Math.round(delta * 100) / 100, whole ? 0 : 2) };
}

/** Movement as a percentage of the parameter's range: parameters of any scale can share one chart. */
export function normalised(p: Pick<Param, "start" | "min" | "max">, v: number): number {
  const span = p.max - p.min;
  return span > 0 ? ((v - p.start) / span) * 100 : 0;
}

/** The chart's data: iteration 0 is the start, then one point per finished iteration. */
export function trajectory(params: readonly Param[], history: readonly Iteration[]) {
  const x = [0, ...history.map((h) => h.iteration)];
  const values = params.map((p, i) => [p.start, ...history.map((h) => h.values[i] ?? p.start)]);
  return { x, values, scaled: params.map((p, i) => values[i]!.map((v) => normalised(p, v))) };
}

/** Game pairs of an iteration from θ+'s side: [LL, LD, WL+DD, WD, WW] as wins, draws and losses. */
export function pairs(penta: readonly number[]) {
  const [ll = 0, ld = 0, mid = 0, wd = 0, ww = 0] = penta;
  return { win: wd + ww, draw: mid, loss: ll + ld, total: ll + ld + mid + wd + ww };
}

/** Games in a whole tune: each pair is two games. */
export const gamesIn = (iterations: number, pairsPer: number) => iterations * pairsPer * 2;

/** What the time control reads as, for the settings line. */
export function controlText(t: Pick<Tune, "spec">): string {
  return timeControl(t.spec.tc as Parameters<typeof timeControl>[0]);
}

/** A rolling estimate of the time per iteration, from how fast this page has seen iterations arrive. */
export function eta(samples: readonly { iteration: number; at: number }[], iterations: number): string | undefined {
  const first = samples[0];
  const last = samples.at(-1);
  if (!first || !last || last.iteration - first.iteration < 2 || last.at - first.at < 15_000) return undefined;
  const perIteration = (last.at - first.at) / (last.iteration - first.iteration);
  return duration(((iterations - last.iteration) * perIteration) / 60_000);
}

/** One plain sentence on where the tune stands. */
export function tuneLine(t: Tune, remaining?: string): string {
  const moved = t.params.filter((p) => Math.abs(change(p, p.value).share) >= 5).length;
  switch (t.status) {
    case "queued":
      return "Waiting for the runner. Tunes queue behind tests, and run one job at a time.";
    case "running":
      return t.iteration
        ? `Tuning: ${t.iteration.toLocaleString()} of ${t.iterations.toLocaleString()} iterations done${remaining ? `, about ${remaining} to go` : ""}.`
        : "Starting: building the engine and reading its parameters.";
    case "done":
      return `Finished all ${t.iterations.toLocaleString()} iterations. ${moved} of ${t.params.length} parameters moved by 5% of their range or more. Trust the confirmation test, not the trajectory.`;
    case "failed":
      return `Failed after ${t.iteration.toLocaleString()} of ${t.iterations.toLocaleString()} iterations${t.summary ? `: ${t.summary}` : "."}`;
    default:
      return `Stopped after ${t.iteration.toLocaleString()} of ${t.iterations.toLocaleString()} iterations. Nothing is lost: it can resume.`;
  }
}

/** The time controls a tune offers: each is a runner preset, or a node count. */
export const controls = [
  { value: "stc", label: "Short games (8+0.08)", detail: "The default, as the tests use." },
  {
    value: "vstc",
    label: "Very short games (4+0.04)",
    detail: "Twice as many iterations per hour; noisier for search tuning.",
  },
  { value: "ltc", label: "Long games (40+0.4)", detail: "Tune at the time control you care about; slow." },
  { value: "nodes", label: "Fixed nodes per move", detail: "Only to check that everything works." },
] as const;
