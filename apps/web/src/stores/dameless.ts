import { ref } from "vue";
import { defineStore } from "pinia";
import { api } from "../api/client";
import type { DamelessOverview, Machine, PruneResult, Status } from "../api/types";
import { listen, reloadOnReconnect } from "./listen";

/**
 * Dameless as a whole: the champion, its line and strength, and the machine it runs on. The overview is read when the
 * Dameless sheet opens and again when a version or an idea changes under it.
 */
export const useDameless = defineStore("dameless", () => {
  const overview = ref<DamelessOverview | null>(null);
  const machine = ref<Machine | null>(null);
  const token = ref<string | null>(null);
  let wanted = false;

  async function load() {
    wanted = true;
    overview.value = await api.get<DamelessOverview>("/api/dameless");
  }

  async function loadMachine() {
    machine.value = (await api.get<Status>("/api/status")).machine ?? null;
  }

  async function loadToken() {
    token.value = (await api.get<{ token: string }>("/api/token")).token;
  }

  /** Pushes the fork's `main` to `origin`: explicit, never automatic. */
  const push = () => api.post<{ remote: string; head: string }>("/api/dameless/push");

  /** Frees the binaries nothing uses; with `dry_run` only says how many and how much. */
  async function prune(dryRun: boolean): Promise<PruneResult> {
    const result = await api.post<PruneResult>("/api/builds/prune", { dry_run: dryRun });
    if (!dryRun) await loadMachine();
    return result;
  }

  const again = () => {
    if (wanted) void load().catch(() => {});
  };
  listen("version.upserted", again);
  listen("idea.upserted", again);
  reloadOnReconnect(async () => again());

  return { overview, machine, token, load, loadMachine, loadToken, push, prune };
});
