import { computed, ref } from "vue";
import { defineStore } from "pinia";
import { client, ok } from "../api/client";
import type { LevelsOverview, PlannedMatch } from "../api/types";
import { useEngines } from "./engines";
import { listen } from "./listen";

/**
 * The engines' strength ladders: every engine that plays on the clock has the same levels (`stockfish@5`,
 * `reckless@3`), each engine with its own ratings and its own level suggested for You. Each ladder is read when first
 * asked for and kept current by `levels.updated`; `levels` is the analysis engine's.
 */
export const useLevels = defineStore("levels", () => {
  const engines = useEngines();
  /** Each engine's ladder, by engine id, as far as read. */
  const byEngine = ref<Record<string, LevelsOverview>>({});
  /** The engine the server answered for when asked for no engine (the analysis engine as it saw it). */
  const fallback = ref<string | null>(null);
  /** The calibration matches that would teach the most now for the engine last planned, with their games. */
  const plan = ref<PlannedMatch[]>([]);

  /** One engine's ladder, once read. */
  const of = (engine: string): LevelsOverview | null => byEngine.value[engine] ?? null;
  /** The analysis engine's ladder. */
  const levels = computed<LevelsOverview | null>(
    () => of(engines.analysisId) ?? (fallback.value ? of(fallback.value) : null),
  );

  /** Reads an engine's ladder (the analysis engine's when none is named). */
  async function loadLevels(engine?: string) {
    const query = engine ? { engine } : {};
    const o = await ok(client.GET("/api/levels", { params: { query } }));
    byEngine.value = { ...byEngine.value, [o.engine]: o };
    if (!engine) fallback.value = o.engine;
    return o;
  }

  /** Reads every engine's ladder not read yet. */
  const loadAll = (ids: readonly string[]) =>
    Promise.all(ids.filter((id) => !byEngine.value[id]).map((id) => loadLevels(id).catch(() => null)));

  async function loadPlan(count = 3, engine?: string) {
    const query = engine ? { count, engine } : { count };
    plan.value = await ok(client.GET("/api/levels/plan", { params: { query } }));
  }

  /** Queues calibration of an engine's levels (background work at the lowest priority); answers the work. */
  const calibrate = (count = 3, engine?: string) =>
    ok(client.POST("/api/levels/calibrate", { body: engine ? { count, engine } : { count } }));

  listen("levels.updated", (u) => {
    if (byEngine.value[u.engine]) void loadLevels(u.engine).catch(() => {});
  });

  return { byEngine, levels, plan, of, loadLevels, loadAll, loadPlan, calibrate };
});
