import { computed, ref, watch } from "vue";
import { defineStore } from "pinia";
import { ApiError, client, ok } from "../api/client";
import type { Prep, Side } from "../api/types";
import { stored } from "../stored";
import { listen, reloadOnReconnect } from "./listen";
import { usePolicy } from "./policy";

/**
 * Your opening prep: the lines you play with one colour, how you score in each and how often you go wrong early. The
 * colour on show is kept here and shared with the repertoire tree, so the two follow one choice. Prep is assistance
 * like the repertoire: the server answers 409 while you play on Lichess, the policy says so, and the section is hidden.
 * A line is read when its colour is first shown, and again when a game of yours ends or a review lands.
 */
export const usePrep = defineStore("prep", () => {
  const policy = usePolicy();
  const side = stored<Side>("repertoire.side", "white");
  const sides = ref<Record<Side, Prep | null>>({ white: null, black: null });
  const loading = ref(false);
  const failed = ref<string | null>(null);
  /** Whether a screen wants it kept current. */
  let wanted = false;

  const available = computed(() => policy.explorer);
  const prep = computed(() => sides.value[side.value]);

  async function read(which: Side) {
    sides.value[which] = await ok(client.GET("/api/players/you/prep", { params: { query: { side: which } } }));
  }

  /** Reads the colour on show. */
  async function load() {
    wanted = true;
    if (!available.value) return;
    loading.value = true;
    failed.value = null;
    try {
      await read(side.value);
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
    sides.value = { white: null, black: null };
    timer = setTimeout(() => void load().catch(() => {}), 1500);
  }
  listen("review.upserted", refresh);
  listen("game.upserted", (g) => {
    if (g.you && g.status === "finished") refresh();
  });
  reloadOnReconnect(async () => {
    if (wanted) await load();
  });
  watch(side, () => {
    if (wanted && !sides.value[side.value]) void load().catch(() => {});
  });
  // Asked for before the policy was known, or while you played: read once it allows.
  watch(available, (now) => {
    if (now && wanted && !prep.value) void load().catch(() => {});
  });

  return { side, sides, prep, loading, failed, available, load };
});
