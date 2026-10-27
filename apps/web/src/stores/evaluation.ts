import { ref, shallowRef } from "vue";
import { defineStore } from "pinia";
import { client, ok, serverFetch } from "../api/client";
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
  /**
   * This tab: the server keeps one session per client and a new one replaces the old, so the id outlives a reload
   * (kept for the tab) and a reloaded page takes over its own search rather than leaving it running.
   */
  const clientId = tabId();
  const end = (s: string) => client.DELETE("/api/eval/{session}", { params: { path: { session: s } } }).catch(() => {});

  async function stop() {
    generation++;
    const s = session.value;
    session.value = null;
    info.value = null;
    target.value = null;
    if (!s) return;
    stream.unfollow(`eval:${s}`);
    await end(s);
  }

  async function start(at: EvalTarget, multipv = 3) {
    await stop();
    const mine = ++generation;
    target.value = at;
    error.value = null;
    try {
      const res = await ok(
        client.POST("/api/eval", {
          body: { fen: at.fen, moves: at.moves, multipv, budget: { type: "infinite" }, client: clientId },
        }),
      );
      if (mine !== generation) {
        await end(res.session);
        return;
      }
      session.value = res.session;
      stream.follow(`eval:${res.session}`);
    } catch (e) {
      if (mine === generation) error.value = e instanceof Error ? e.message : String(e);
    }
  }

  // A closed or reloaded tab stops its search: `keepalive` lets the request outlive the page.
  addEventListener("pagehide", () => {
    const s = session.value;
    if (s) void serverFetch(`/api/eval/${s}`, { method: "DELETE", keepalive: true }).catch(() => {});
  });

  listen("eval.info", (d) => {
    if (d.session === session.value) info.value = d;
  });

  return { session, info, target, error, start, stop };
});

const TAB_KEY = "dameless.tab";

/** An id for this browser tab, kept in its session storage (a fresh one when that is not available). */
function tabId(): string {
  const fresh = `tab-${Math.random().toString(36).slice(2, 10)}`;
  try {
    const kept = sessionStorage.getItem(TAB_KEY);
    if (kept) return kept;
    sessionStorage.setItem(TAB_KEY, fresh);
  } catch {}
  return fresh;
}
