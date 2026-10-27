import { computed, ref } from "vue";
import { defineStore } from "pinia";
import { ApiError, client, ok } from "../api/client";
import type { Repertoire, Side } from "../api/types";
import { listen, reloadOnReconnect } from "./listen";
import { usePolicy } from "./policy";

/**
 * Your openings, from your games: one repertoire per colour. Built from your games with Lichess's book for theory, so
 * like the explorer it is off while you play on Lichess (the server answers 409 and the policy says so). Read when a
 * screen that shows it opens, and again when a game of yours ends or a review lands.
 */
export const useRepertoire = defineStore("repertoire", () => {
  const policy = usePolicy();
  const sides = ref<Record<Side, Repertoire | null>>({ white: null, black: null });
  const loading = ref(false);
  const failed = ref<string | null>(null);
  /** Whether a screen wants it kept current. */
  let wanted = false;

  /** Hidden while the policy forbids the explorer. */
  const available = computed(() => policy.explorer);
  const empty = computed(() => sides.value.white?.games === 0 && sides.value.black?.games === 0);

  async function load() {
    wanted = true;
    if (!available.value) return;
    loading.value = true;
    failed.value = null;
    try {
      const one = (side: Side) => ok(client.GET("/api/players/you/repertoire", { params: { query: { side } } }));
      const [white, black] = await Promise.all([one("white"), one("black")]);
      sides.value = { white, black };
    } catch (e) {
      // A fair-play refusal is not a failure: the policy hides the section.
      if (!(e instanceof ApiError && e.code === "fair_play")) failed.value = e instanceof Error ? e.message : String(e);
    } finally {
      loading.value = false;
    }
  }

  let timer: ReturnType<typeof setTimeout> | undefined;
  function refresh() {
    if (!wanted) return;
    clearTimeout(timer);
    timer = setTimeout(() => void load().catch(() => {}), 1500);
  }
  listen("review.upserted", refresh);
  listen("game.upserted", (g) => {
    if (g.you && g.status === "finished") refresh();
  });
  reloadOnReconnect(async () => {
    if (wanted) await load();
  });

  return { sides, loading, failed, available, empty, load };
});
