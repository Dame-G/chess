import { ref, shallowRef } from "vue";
import { defineStore } from "pinia";
import { api } from "../api/client";
import { stream } from "../api/stream";
import type { EvalInfo } from "../api/types";
import { listen } from "./listen";

/** What the evaluation is of: a start and the moves played from it, as the server takes it. */
export interface EvalTarget {
  fen: string;
  moves: string[];
}

/**
 * The board's live evaluation: one session at a time (`POST /api/eval`), its lines arriving on `eval:<session>`.
 * Starting it on a new position stops the old session first.
 */
export const useEvaluation = defineStore("evaluation", () => {
  const session = ref<string | null>(null);
  const info = shallowRef<EvalInfo | null>(null);
  /** The position the info is for, so a late line for the previous one is never shown against the new. */
  const target = shallowRef<EvalTarget | null>(null);
  const error = ref<string | null>(null);
  let generation = 0;

  async function stop() {
    generation++;
    const s = session.value;
    session.value = null;
    info.value = null;
    target.value = null;
    if (!s) return;
    stream.unfollow(`eval:${s}`);
    await api.delete(`/api/eval/${s}`).catch(() => {});
  }

  async function start(at: EvalTarget, multipv = 3) {
    await stop();
    const mine = ++generation;
    target.value = at;
    error.value = null;
    try {
      const res = await api.post<{ session: string }>("/api/eval", {
        fen: at.fen,
        moves: at.moves,
        multipv,
        budget: { type: "infinite" },
      });
      if (mine !== generation) {
        await api.delete(`/api/eval/${res.session}`).catch(() => {});
        return;
      }
      session.value = res.session;
      stream.follow(`eval:${res.session}`);
    } catch (e) {
      if (mine === generation) error.value = e instanceof Error ? e.message : String(e);
    }
  }

  listen("eval.info", (d) => {
    if (d.session === session.value) info.value = d;
  });

  return { session, info, target, error, start, stop };
});
