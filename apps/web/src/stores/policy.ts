import { computed, ref } from "vue";
import { defineStore } from "pinia";
import { client, ok } from "../api/client";
import type { Policy, Status } from "../api/types";
import { listen, reloadOnReconnect } from "./listen";

/**
 * Fair play, as the server decides it. While You play on Lichess, analysis is not on screen: components read
 * `allowed` (engine, eval bar, graph, reviews) and `explorer` (the opening explorer) and hide what they may not show.
 */
export const usePolicy = defineStore("policy", () => {
  const policy = ref<Policy>({ assist: "allowed", reason: null });
  const status = ref<Status | null>(null);
  /** Nothing is allowed until the server has said: a page that opens during a Lichess game asks for no analysis. */
  const known = ref(false);

  const allowed = computed(() => known.value && policy.value.assist === "allowed");
  const explorer = computed(() => known.value && policy.value.assist !== "off");
  const reason = computed(() =>
    !known.value || policy.value.assist === "allowed"
      ? null
      : (policy.value.reason ?? "Analysis is off while you're playing on Lichess"),
  );

  async function load() {
    status.value = await ok(client.GET("/api/status"));
    policy.value = status.value.policy;
    known.value = true;
  }

  listen("policy.changed", (p) => {
    policy.value = p;
    known.value = true;
  });
  reloadOnReconnect(load);

  return { policy, status, known, allowed, explorer, reason, load };
});
