import { computed, ref } from "vue";
import { defineStore } from "pinia";
import { api } from "../api/client";
import type { Policy, Status } from "../api/types";
import { listen, reloadOnReconnect } from "./listen";

/**
 * Fair play, as the server decides it. While You play on Lichess, analysis is not on screen: components read
 * `allowed` (engine, eval bar, graph, reviews) and `explorer` (the opening explorer) and hide what they may not show.
 */
export const usePolicy = defineStore("policy", () => {
  const policy = ref<Policy>({ assist: "allowed", reason: null });
  const status = ref<Status | null>(null);

  const allowed = computed(() => policy.value.assist === "allowed");
  const explorer = computed(() => policy.value.assist !== "off");
  const reason = computed(() =>
    allowed.value ? null : (policy.value.reason ?? "Analysis is off while you're playing on Lichess"),
  );

  async function load() {
    status.value = await api.get<Status>("/api/status");
    policy.value = status.value.policy;
  }

  listen("policy.changed", (p) => {
    policy.value = p;
  });
  reloadOnReconnect(load);

  return { policy, status, allowed, explorer, reason, load };
});
