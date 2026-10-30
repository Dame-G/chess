import { computed, ref } from "vue";
import { defineStore } from "pinia";
import { client, ok } from "../api/client";
import type { Engine, EnginesOverview, NewEngineRequest, SetupPlan, SetupRequest } from "../api/types";
import { listen, reloadOnReconnect } from "./listen";

/** The engine the app analyses with until the server has said: the built-in Stockfish. */
const DEFAULT_ANALYSIS = "stockfish";

/**
 * The engines list: the built-ins (Stockfish and Reckless, installed or not) and the engines you added, with the
 * one that analyses. Loaded once and kept current by `engines.changed`.
 */
export const useEngines = defineStore("engines", () => {
  const overview = ref<EnginesOverview | null>(null);
  const loaded = ref(false);
  /** What setup would download, read when Settings asks. */
  const plan = ref<SetupPlan | null>(null);

  const list = computed<Engine[]>(() => overview.value?.engines ?? []);
  /** The analysis engine's id: reviews, live evaluation, drills and the levels run on it. */
  const analysisId = computed(() => overview.value?.analysis ?? DEFAULT_ANALYSIS);
  const analysis = computed(() => list.value.find((e) => e.id === analysisId.value) ?? null);
  const byId = (id: string) => list.value.find((e) => e.id === id);

  async function load() {
    overview.value = await ok(client.GET("/api/engines"));
    loaded.value = true;
  }

  /** Adds an engine (a UCI binary on this machine); the server checks it answers. */
  async function add(body: NewEngineRequest) {
    const engine = await ok(client.POST("/api/engines", { body }));
    await load();
    return engine;
  }

  async function remove(id: string) {
    overview.value = await ok(client.DELETE("/api/engines/{id}", { params: { path: { id } } }));
  }

  async function useForAnalysis(engine: string) {
    overview.value = await ok(client.PUT("/api/engines/analysis", { body: { engine } }));
  }

  async function loadPlan() {
    plan.value = await ok(client.GET("/api/setup"));
  }

  /** Downloads the built-in engines, as work: the work indicator shows it. */
  const setup = (body: SetupRequest = {}) => ok(client.POST("/api/setup", { body }));

  listen("engines.changed", (o) => {
    overview.value = o;
    loaded.value = true;
    if (plan.value) void loadPlan().catch(() => {});
  });
  reloadOnReconnect(load);

  return {
    overview,
    loaded,
    plan,
    list,
    analysisId,
    analysis,
    byId,
    load,
    add,
    remove,
    useForAnalysis,
    loadPlan,
    setup,
  };
});
