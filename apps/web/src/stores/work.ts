import { computed, ref } from "vue";
import { defineStore } from "pinia";
import { api } from "../api/client";
import type { Priority, Work, WorkId } from "../api/types";
import { listen, reloadOnReconnect } from "./listen";

const RANK: Record<Priority, number> = { live: 0, interactive: 1, review: 2, idea: 3 };

/** Background work: running, queued (in the order the scheduler takes it) and recently ended. */
export const useWork = defineStore("work", () => {
  const items = ref<Record<WorkId, Work>>({});
  const loaded = ref(false);

  const all = computed(() => Object.values(items.value));
  const running = computed(() =>
    all.value.filter((w) => w.status === "running" || w.status === "paused").sort((a, b) => a.id - b.id),
  );
  const queued = computed(() =>
    all.value
      .filter((w) => w.status === "queued")
      .sort((a, b) => RANK[a.priority] - RANK[b.priority] || a.created_at.localeCompare(b.created_at)),
  );
  const recent = computed(() =>
    all.value
      .filter((w) => w.status === "done" || w.status === "failed" || w.status === "stopped")
      .sort((a, b) => (b.ended_at ?? "").localeCompare(a.ended_at ?? ""))
      .slice(0, 8),
  );

  async function load() {
    const list = await api.get<Work[]>("/api/work");
    const next: Record<WorkId, Work> = {};
    for (const w of list) next[w.id] = w;
    items.value = next;
    loaded.value = true;
  }

  const stop = (id: WorkId) => api.post(`/api/work/${id}/stop`);
  const pause = (id: WorkId) => api.post(`/api/work/${id}/pause`);
  const resume = (id: WorkId) => api.post(`/api/work/${id}/resume`);

  listen("work.upserted", (w) => {
    items.value[w.id] = w;
  });
  reloadOnReconnect(load);

  return { items, running, queued, recent, loaded, load, stop, pause, resume };
});
