import { computed, ref } from "vue";
import { defineStore } from "pinia";
import { api } from "../api/client";
import type { Idea, IdeaId, Trial, TrialId, TrialSummary, TuneProgress } from "../api/types";
import { listen, reloadOnReconnect } from "./listen";

const summaryOf = (t: Trial): TrialSummary => ({
  id: t.id,
  stage: t.stage,
  status: t.status,
  time: t.time,
  penta: t.penta,
  llr: t.llr,
  elo: t.elo,
  detail: t.detail,
});

/** Every idea with its trials' summaries, full trials as they are fetched or sent, and each tune's latest step. */
export const useIdeas = defineStore("ideas", () => {
  const ideas = ref<Record<IdeaId, Idea>>({});
  const trials = ref<Record<TrialId, Trial>>({});
  const tunes = ref<Record<IdeaId, TuneProgress>>({});
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
    const all = await api.get<Idea[]>("/api/ideas");
    const next: Record<IdeaId, Idea> = {};
    for (const i of all) next[i.id] = i;
    ideas.value = next;
    loaded.value = true;
  }

  async function loadTrial(idea: IdeaId, trial: TrialId) {
    trials.value[trial] = await api.get<Trial>(`/api/ideas/${idea}/trials/${trial}`);
  }

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
    const t = trials.value[p.id];
    if (t) Object.assign(t, { penta: p.penta, wdl: p.wdl, llr: p.llr, elo: p.elo });
    patchSummary(p.id, { penta: p.penta, llr: p.llr, elo: p.elo });
  });
  listen("tune.progress", (p) => {
    tunes.value[p.idea] = p;
  });
  reloadOnReconnect(load);

  return { ideas, trials, tunes, list, running, queued, loaded, ofTrial, load, loadTrial };
});
