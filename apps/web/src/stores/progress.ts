import { ref } from "vue";
import { defineStore } from "pinia";
import { client, ok } from "../api/client";
import type { ProgressChart } from "../api/types";
import { listen, reloadOnReconnect } from "./listen";

/** Progress: your rating, accuracy and drill hit rate over time beside Dameless's Elo over its champion versions. */
export const useProgress = defineStore("progress", () => {
  const chart = ref<ProgressChart | null>(null);
  let wanted = false;

  async function load() {
    wanted = true;
    chart.value = await ok(client.GET("/api/progress", { params: { query: { window: 10 } } }));
  }

  let timer: ReturnType<typeof setTimeout> | undefined;
  function refresh() {
    if (!wanted) return;
    clearTimeout(timer);
    timer = setTimeout(() => void load().catch(() => {}), 1500);
  }
  listen("review.upserted", refresh);
  listen("version.upserted", refresh);
  listen("drill.answered", refresh);
  listen("levels.updated", refresh);
  reloadOnReconnect(async () => {
    if (wanted) await load();
  });

  return { chart, load };
});
