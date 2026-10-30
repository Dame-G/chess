import { ref } from "vue";
import { defineStore } from "pinia";
import { client, ok } from "../api/client";
import type { LevelsOverview, PlannedMatch } from "../api/types";
import { listen } from "./listen";

/** The strength ladder (`dameless@<level>` seats) with each level's rating, and the level suggested for You. */
export const useLevels = defineStore("levels", () => {
  const levels = ref<LevelsOverview | null>(null);
  /** The calibration matches that would teach the most now, with how many games each plays. */
  const plan = ref<PlannedMatch[]>([]);

  async function loadLevels() {
    levels.value = await ok(client.GET("/api/levels"));
  }

  async function loadPlan(count = 3) {
    plan.value = await ok(client.GET("/api/levels/plan", { params: { query: { count } } }));
  }

  /** Queues calibration of the levels (background work at the lowest priority); answers the work. */
  const calibrate = (count = 3) => ok(client.POST("/api/levels/calibrate", { body: { count } }));

  listen("levels.updated", () => {
    if (levels.value) void loadLevels().catch(() => {});
  });

  return { levels, plan, loadLevels, loadPlan, calibrate };
});
