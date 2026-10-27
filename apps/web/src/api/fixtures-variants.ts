/**
 * Fixture variants for the Dameless side, for reviewing states the shared fixtures do not reach. Ask for one with
 * `?fixtures=<name>`, or several joined by `+` (`?fixtures=many+untrusted`):
 *
 * - `empty`: no ideas, no trials, and a champion line of one version
 * - `loading`: the Dameless overview, the ideas, the networks and the data never answer
 * - `error`: the same routes answer 500
 * - `long`: an idea whose title and branch are 60 characters, with long tags, notes and a long verdict
 * - `five`: an idea with all five trials, the last one running with live progress
 * - `many`: 200 ideas of every status
 * - `tune0`, `tune5000`: the running tune at iteration 0, and at iteration 5,000
 * - `nogpu`: a machine with no GPU
 * - `untrusted`: a harness that has not been checked on this machine
 *
 * Everything here is invented, like the rest of the fixtures.
 */
import { error } from "./fixtures-http";
import { fixtureMode } from "./mode";
import type { SheetContext } from "./fixtures-sheets";
import type { Idea, IdeaStatus, Trial, TimeControl } from "./types";

/** Whether a variant was asked for. */
export const variant = (name: string): boolean => (fixtureMode() ?? "").split("+").includes(name);

const pending = () => new Promise<never>(() => {});
const DATA_ROUTES = /^\/api\/(dameless|ideas|nets|data)(\/|$)/;

/** What `loading` and `error` do to a request: a read of the Dameless side hangs or fails. */
export function variantResponse(method: string, path: string): Response | Promise<Response> | undefined {
  if (method !== "GET" || !DATA_ROUTES.test(path)) return undefined;
  if (variant("loading")) return pending();
  if (variant("error")) return error(500, "internal", "The app could not read its database: the file is locked.");
  return undefined;
}

/** The tune's length under `tune0` and `tune5000`: steps taken and iterations planned, or null for the default. */
export function tuneVariant(): { steps: number; iterations: number } | null {
  if (variant("tune0")) return { steps: 0, iterations: 1000 };
  if (variant("tune5000")) return { steps: 5000, iterations: 10_000 };
  return null;
}

const variantTune = tuneVariant;

const stc: TimeControl = { type: "clock", initial_ms: 8000, increment_ms: 80 };

const baseTrial = (t: Partial<Trial> & Pick<Trial, "id" | "idea" | "stage" | "status">): Trial => ({
  preset: null,
  time: null,
  played: null,
  scaling: null,
  bounds: null,
  penta: [0, 0, 0, 0, 0],
  wdl: [0, 0, 0],
  llr: null,
  elo: null,
  detail: null,
  nelo: null,
  started_at: null,
  ended_at: null,
  ...t,
});

const baseIdea = (i: Partial<Idea> & Pick<Idea, "id" | "title" | "kind" | "branch" | "status">): Idea => ({
  hypothesis: "",
  author: "claude",
  base: 14,
  version: null,
  verdict: null,
  trials: [],
  tags: [],
  notes: null,
  created_at: new Date().toISOString(),
  ended_at: null,
  ...i,
});

const LONG_TITLE = "Scale the late move reduction by the square root of depth for quiet moves"; // 71
const LONG_BRANCH = "experiments/late-move-reduction-scaled-by-the-root-of-search-depth"; // 63

/** Adds the data a variant asks for to the shared ideas and trials. */
export function applyVariants(ctx: SheetContext): void {
  const minutes = (m: number) => ctx.minutesAgo(m);
  const nextTrial = () => Math.max(0, ...ctx.trials.map((t) => t.id)) + 1;
  const add = (idea: Idea, trials: Trial[]) => {
    ctx.trials.push(...trials);
    idea.trials = trials.map(ctx.summaryOf);
    ctx.ideas.push(idea);
  };

  const tune = variantTune();
  if (tune) {
    const t = ctx.trials.find((x) => x.id === 20);
    if (t) t.detail = `Iteration ${tune.steps.toLocaleString("en-GB")} of ${tune.iterations.toLocaleString("en-GB")}.`;
    for (const i of ctx.ideas) i.trials = ctx.trials.filter((x) => x.idea === i.id).map(ctx.summaryOf);
  }

  if (variant("empty")) {
    ctx.ideas.length = 0;
    ctx.trials.length = 0;
  }

  if (variant("long")) {
    const t0 = nextTrial();
    add(
      baseIdea({
        id: 40,
        title: LONG_TITLE.slice(0, 60),
        kind: "gain",
        branch: LONG_BRANCH.slice(0, 60),
        status: "failed",
        hypothesis:
          "Reducing quiet late moves by a depth-scaled amount, instead of a fixed table, should save nodes at high depth without losing the tactics that the shallow reductions find, and it removes two constants.",
        verdict: "Failed: the branch did not build, cannot find value `reduction_scaled_by_root_depth` in this scope.",
        tags: ["search-experiments-october", "late-move-reduction", "depth-scaling-and-friends"],
        notes:
          "A long note with `a_very_long_identifier_that_cannot_break_anywhere_at_all_in_a_narrow_column` in it.\n\n- one\n- two",
        created_at: minutes(12),
        ended_at: minutes(10),
      }),
      [
        baseTrial({
          id: t0,
          idea: 40,
          stage: "fingerprint",
          status: "failed",
          detail: "The branch did not build: cannot find value `reduction_scaled_by_root_depth` in this scope.",
          ended_at: minutes(10),
        }),
      ],
    );
  }

  if (variant("five")) {
    const t0 = nextTrial();
    const id = 41;
    const done = (i: number, stage: Trial["stage"], preset: Trial["preset"], elo: number, pairs: number) =>
      baseTrial({
        id: t0 + i,
        idea: id,
        stage,
        status: "passed",
        preset,
        time: stc,
        bounds: [0, 3],
        penta: [
          Math.round(pairs * 0.04),
          Math.round(pairs * 0.2),
          Math.round(pairs * 0.52),
          Math.round(pairs * 0.19),
          Math.round(pairs * 0.05),
        ],
        wdl: [Math.round(pairs * 0.9), Math.round(pairs * 1.3), Math.round(pairs * 0.8)],
        llr: { value: 2.95, lower: -2.94, upper: 2.94 },
        elo: { elo, low: elo - 3, high: elo + 3 },
        nelo: { elo: elo * 1.6, low: elo * 1.6 - 5, high: elo * 1.6 + 5 },
        detail: `Kept at ${pairs.toLocaleString("en-GB")} pairs: +${elo.toFixed(1)} ± 3.0 Elo.`,
        started_at: minutes(300),
        ended_at: minutes(200),
      });
    const live = baseTrial({
      id: t0 + 4,
      idea: id,
      stage: "long",
      status: "running",
      preset: "ltc",
      time: { type: "clock", initial_ms: 40_000, increment_ms: 400 },
      bounds: [0, 3],
      penta: [10, 58, 160, 61, 11],
      wdl: [300, 420, 280],
      llr: { value: 0.8, lower: -2.94, upper: 2.94 },
      elo: { elo: 1.4, low: -2.9, high: 5.7 },
      detail: "300 pairs so far: +1.4 ± 4.3 Elo.",
      started_at: minutes(20),
    });
    add(
      baseIdea({
        id,
        title: "History bonus by stage",
        kind: "net",
        branch: "history-stage",
        status: "running",
        hypothesis: "Scaling the history bonus by the game stage helps endings.",
        created_at: minutes(320),
      }),
      [
        baseTrial({
          id: t0,
          idea: id,
          stage: "train",
          status: "passed",
          detail: "Trained smoke-2 on 65.5 million positions.",
          ended_at: minutes(320),
        }),
        baseTrial({
          id: t0 + 5,
          idea: id,
          stage: "fingerprint",
          status: "passed",
          detail: "Bench changed.",
          ended_at: minutes(300),
        }),
        done(1, "screen", "vstc", 2.1, 1400),
        baseTrial({
          id: t0 + 3,
          idea: id,
          stage: "short",
          status: "passed",
          preset: "stc",
          time: stc,
          bounds: [0, 3],
          penta: [90, 600, 1650, 640, 100],
          wdl: [2000, 3000, 1900],
          llr: { value: 2.96, lower: -2.94, upper: 2.94 },
          elo: { elo: 1.9, low: -0.4, high: 4.2 },
          detail: "Kept at 3,080 pairs: +1.9 ± 2.3 Elo.",
          ended_at: minutes(150),
        }),
        live,
      ],
    );
    let step = 0;
    setInterval(() => {
      step += 1;
      const k = step % 5;
      live.penta[k] = (live.penta[k] ?? 0) + 1;
      const pairs = live.penta.reduce((a, b) => a + b, 0);
      live.llr = {
        value: Math.round((live.llr!.value + (Math.sin(step) + 0.3) * 0.04) * 100) / 100,
        lower: -2.94,
        upper: 2.94,
      };
      live.detail = `${pairs} pairs so far: +1.4 ± 4.3 Elo.`;
      ctx.emit("trial.progress", `idea:${id}`, {
        id: live.id,
        idea: id,
        penta: [...live.penta],
        wdl: live.wdl,
        llr: live.llr,
        elo: live.elo,
      });
    }, 1500);
  }

  if (variant("many")) {
    const statuses: IdeaStatus[] = ["kept", "dropped", "dropped", "stopped", "failed", "dropped", "kept"];
    const kinds = ["gain", "neutral", "speed", "tune", "net"] as const;
    for (let n = 0; n < 200; n++) {
      const status = n < 3 ? (n === 0 ? "running" : "queued") : statuses[n % statuses.length]!;
      const t0 = nextTrial();
      const idea = baseIdea({
        id: 100 + n,
        title: `Idea ${n + 1}: ${["razoring", "futility", "history", "aspiration", "singular", "nullmove"][n % 6]} variant ${Math.floor(n / 6)}`,
        kind: kinds[n % kinds.length]!,
        branch: `idea-${n}`,
        status,
        hypothesis: "One of many.",
        author: n % 3 === 0 ? "you" : "claude",
        created_at: minutes(60 + n * 37),
        ended_at: status === "running" || status === "queued" ? null : minutes(50 + n * 37),
      });
      const elo = ((n * 7) % 19) / 4 - 2.5;
      add(idea, [
        baseTrial({
          id: t0,
          idea: idea.id,
          stage: "short",
          status: status === "running" ? "running" : status === "failed" ? "failed" : "passed",
          bounds: [0, 3],
          penta: [4, 30, 70, 28, 5],
          wdl: [200, 260, 210],
          llr: { value: elo / 2, lower: -2.94, upper: 2.94 },
          elo: { elo, low: elo - 4, high: elo + 4 },
        }),
      ]);
    }
  }
}
