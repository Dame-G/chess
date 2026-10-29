import { computed, ref } from "vue";
import { defineStore } from "pinia";
import { client, ok } from "../api/client";
import type {
  Idea,
  IdeaId,
  NewIdeaRequest,
  Trial,
  TrialId,
  TrialSummary,
  TuneProgress,
  TuneDetail,
} from "../api/types";
import { pairsOf } from "../sheets/idea";
import { listen, reloadOnReconnect } from "./listen";

const summaryOf = (t: Trial): TrialSummary => ({
  id: t.id,
  stage: t.stage,
  status: t.status,
  preset: t.preset ?? null,
  bounds: t.bounds ?? null,
  pairs: pairsOf(t.penta),
  llr: t.llr ?? null,
  elo: t.elo ?? null,
  detail: t.detail ?? null,
});

/** Every idea with its trials' summaries, full trials as they are fetched or sent, and each tune's latest step. */
export const useIdeas = defineStore("ideas", () => {
  const ideas = ref<Record<IdeaId, Idea>>({});
  const trials = ref<Record<TrialId, Trial>>({});
  const tunes = ref<Record<IdeaId, TuneProgress>>({});
  /** Each tune idea's spec, parameters and steps, as they are fetched and then extended by `tune.progress`. */
  const tuneStates = ref<Record<IdeaId, TuneDetail>>({});
  /** Each trial's evidence over the pairs played, [pairs, LLR], collected from `trial.progress` while the app is open. */
  const llrHistory = ref<Record<TrialId, [number, number][]>>({});
  const loaded = ref(false);

  /** Newest first. */
  const list = computed(() => Object.values(ideas.value).sort((a, b) => b.created_at.localeCompare(a.created_at)));
  const running = computed(() => list.value.filter((i) => i.status === "running"));
  const queued = computed(() => list.value.filter((i) => i.status === "queued"));

  /** The idea a trial belongs to, with the trial's summary. */
  function ofTrial(id: TrialId): { idea: Idea; trial: TrialSummary } | undefined {
    for (const idea of Object.values(ideas.value)) {
      const trial = idea.trials.find((t) => t.id === id);
      if (trial) return { idea, trial };
    }
    return undefined;
  }

  async function load() {
    const all = await ok(client.GET("/api/ideas"));
    const next: Record<IdeaId, Idea> = {};
    for (const i of all) next[i.id] = i;
    ideas.value = next;
    loaded.value = true;
  }

  async function loadTrial(idea: IdeaId, trial: TrialId) {
    const t = await ok(client.GET("/api/ideas/{id}/trials/{trial}", { params: { path: { id: idea, trial } } }));
    trials.value[trial] = t;
    const pairs = pairsOf(t.penta);
    if (t.llr && pairs && !llrHistory.value[trial]) llrHistory.value[trial] = [[pairs, t.llr.value]];
  }

  /** Reads one idea in full; the list may not have it yet (a link opened cold). */
  async function loadIdea(id: IdeaId) {
    ideas.value[id] = await ok(client.GET("/api/ideas/{id}", { params: { path: { id } } }));
  }

  async function loadTune(id: IdeaId) {
    tuneStates.value[id] = await ok(client.GET("/api/ideas/{id}/tune", { params: { path: { id } } }));
  }

  /** The tunables a branch declares, to choose from when starting a tune. */
  const tunables = (branch: string) => ok(client.GET("/api/tunables", { params: { query: { branch } } }));

  /** A branch's tip and whether its `Bench:` line matches (never builds): what the New idea form shows as you type. */
  const checkBranch = (branch: string) => ok(client.GET("/api/ideas/check-branch", { params: { query: { branch } } }));

  async function create(idea: NewIdeaRequest): Promise<Idea> {
    const made = await ok(client.POST("/api/ideas", { body: idea }));
    ideas.value[made.id] = made;
    return made;
  }

  const stop = (id: IdeaId) => ok(client.POST("/api/ideas/{id}/stop", { params: { path: { id } } }));
  const resume = (id: IdeaId) => ok(client.POST("/api/ideas/{id}/resume", { params: { path: { id } } }));
  /** Makes a kept idea's branch the new champion; answers the new champion version. */
  const promote = (id: IdeaId) => ok(client.POST("/api/ideas/{id}/promote", { params: { path: { id } } }));

  function patchSummary(id: TrialId, patch: Partial<TrialSummary>) {
    for (const idea of Object.values(ideas.value)) {
      const i = idea.trials.findIndex((x) => x.id === id);
      if (i >= 0) {
        idea.trials[i] = { ...idea.trials[i]!, ...patch };
        return;
      }
    }
  }

  listen("idea.upserted", (idea) => {
    ideas.value[idea.id] = idea;
  });
  listen("trial.upserted", (t) => {
    trials.value[t.id] = t;
    const idea = ideas.value[t.idea];
    if (idea && !idea.trials.some((x) => x.id === t.id)) idea.trials.push(summaryOf(t));
    else patchSummary(t.id, summaryOf(t));
  });
  listen("trial.progress", (p) => {
    if (p.llr) {
      const h = (llrHistory.value[p.id] ??= []);
      const pairs = pairsOf(p.penta);
      if (h.at(-1)?.[0] !== pairs) h.push([pairs, p.llr.value]);
      if (h.length > 2000) h.splice(0, h.length - 2000);
    }
    const t = trials.value[p.id];
    if (t) Object.assign(t, { penta: p.penta, wdl: p.wdl, llr: p.llr, elo: p.elo });
    patchSummary(p.id, { pairs: pairsOf(p.penta), llr: p.llr ?? null, elo: p.elo ?? null });
  });
  listen("tune.progress", (p) => {
    tunes.value[p.idea] = p;
    const state = tuneStates.value[p.idea];
    if (!state || state.steps.some((x) => x.iteration === p.iteration)) return;
    state.steps.push({ iteration: p.iteration, values: p.values, result: p.result, penta: [0, 0, 0, 0, 0] });
  });
  reloadOnReconnect(load);

  return {
    ideas,
    trials,
    tunes,
    tuneStates,
    llrHistory,
    list,
    running,
    queued,
    loaded,
    ofTrial,
    load,
    loadIdea,
    loadTrial,
    loadTune,
    tunables,
    checkBranch,
    create,
    stop,
    resume,
    promote,
  };
});
