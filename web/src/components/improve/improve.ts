import type { Experiment, Job, Tune } from "../../api/queries";
import { stageLabel, verdict } from "../../format";
import { headline } from "../lab/lab";
import { live as tuneLive, statusWord } from "../tune/tune";

/** Everything being tried, in one list: a test of a branch, or a tune that writes one. */
export type Item =
  | { kind: "test"; id: number; at: string; live: boolean; test: Experiment }
  | { kind: "tune"; id: number; at: string; live: boolean; tune: Tune };

export type Tone = "success" | "danger" | "info" | "neutral";

/** The address of one item. */
export const pathOf = (kind: Item["kind"], id: number) => `/improve/${kind === "test" ? "experiments" : "tunes"}/${id}`;

export const keyOf = (item: Pick<Item, "kind" | "id">) => `${item.kind}:${item.id}`;

const running = (e: Pick<Experiment, "status">) => e.status === "queued" || e.status === "running";

/** Tests and tunes together: what is live first, then the newest. */
export function itemsOf(tests: readonly Experiment[], tunes: readonly Tune[]): Item[] {
  const all: Item[] = [
    ...tests.map((test): Item => ({ kind: "test", id: test.id, at: test.created_at, live: running(test), test })),
    ...tunes.map((tune): Item => ({ kind: "tune", id: tune.id, at: tune.created_at, live: tuneLive(tune), tune })),
  ];
  return all.sort((a, b) => Number(b.live) - Number(a.live) || b.at.localeCompare(a.at));
}

/** The words a tune's test has in its name: "tune 3: spsa/tunables". */
const TUNE_NAME = /^tune (\d+):/;

type ApplyResult = { branch?: string; experiment?: number | null };
const applyResult = (job: Job) =>
  job.result && typeof job.result === "object" ? (job.result as ApplyResult) : undefined;

/** The newest job that applied a tune to a branch. */
export const applyJobOf = (tune: Pick<Tune, "id">, jobs: readonly Job[]) =>
  jobs.find((j) => j.spec.kind === "apply_tune" && j.spec.id === tune.id);

/** The tune that a test confirms, when the test came from applying one. */
export function tuneOfTest(e: Experiment, jobs: readonly Job[], tunes: readonly Tune[]): Tune | undefined {
  const job = jobs.find((j) => j.spec.kind === "apply_tune" && applyResult(j)?.experiment === e.id);
  const id = job?.spec.kind === "apply_tune" ? job.spec.id : Number(TUNE_NAME.exec(e.name)?.[1]);
  return tunes.find((t) => t.id === id);
}

/** The test of a tune's branch, when it was applied with one. */
export function testOfTune(t: Tune, jobs: readonly Job[], tests: readonly Experiment[]): Experiment | undefined {
  const made = jobs
    .filter((j) => j.spec.kind === "apply_tune" && j.spec.id === t.id)
    .map((j) => applyResult(j)?.experiment)
    .find((id): id is number => typeof id === "number");
  return tests.find((e) => e.id === made) ?? tests.find((e) => TUNE_NAME.exec(e.name)?.[1] === String(t.id));
}

/** What a row says: a status in a word, a figure behind it, and the colour of both. */
export interface Status {
  label: string;
  figure: string;
  tone: Tone;
}

/** A test, as a layman asks it: did it pass? */
export function testStatus(e: Experiment): Status {
  const v = verdict(e);
  const { figure } = headline(e);
  switch (e.status) {
    case "queued":
      return { label: "Waiting", figure: "", tone: "neutral" };
    case "running":
      return { label: "Running", figure: e.current ? stageLabel(e.current).toLowerCase() : "", tone: "info" };
    case "cancelled":
      return { label: "Stopped", figure: "", tone: "neutral" };
    case "passed":
      return { label: "Passed", figure, tone: "success" };
    default:
      return { label: "Rejected", figure, tone: v.tone === "danger" ? "danger" : "neutral" };
  }
}

/** A tune: how far along it is, and once applied, how its test went. */
export function tuneStatus(t: Tune, test?: Experiment, applied = false): Status {
  if (t.status === "queued") return { label: "Waiting", figure: "", tone: "neutral" };
  if (t.status === "running") return { label: "Tuning", figure: `${t.iteration}/${t.iterations}`, tone: "info" };
  if (test) {
    const s = testStatus(test);
    return { label: `Applied · ${s.label}`, figure: s.figure, tone: s.tone };
  }
  if (applied) return { label: "Applied", figure: "", tone: "success" };
  return { label: statusWord(t), figure: "", tone: t.status === "failed" ? "danger" : "neutral" };
}

/** The colour token behind a row's mark. */
export const toneToken = (tone: Tone) => (tone === "info" ? "primary" : tone === "neutral" ? "border-strong" : tone);
