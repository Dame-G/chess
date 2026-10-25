import { onBeforeUnmount, reactive, ref, watch, type Ref } from "vue";
import { useRunnerEvent } from "../api/events";
import { api } from "../api/runner";
import { unwrap, useStatus } from "../api/queries";
import type { components } from "../api/schema";
import { whitePawns, whiteScore, type Position } from "./position";

type Info = components["schemas"]["Info"];

export interface Seen {
  pawns: number;
  text: string;
  depth: number;
}

/**
 * The live search on the board: starts the engine on the position, restarts a moment after it changes, and keeps the
 * newest report of each line and the deepest evaluation seen for each position.
 */
export function useAnalysis(input: {
  /** Where the line starts: a FEN or `startpos`. */
  start: Ref<string>;
  position: Ref<Position>;
  /** The build to analyse with, as an id. */
  build: Ref<string>;
  multipv: Ref<string>;
  /** Whether a search should be running. */
  active: Ref<boolean>;
}) {
  const { state: status } = useStatus();
  const session = ref<number | null>(null);
  const lines = ref<Info[]>([]);
  const error = ref("");
  /** The best evaluation seen for each position, so the bar and the graph keep what the engine found. */
  const evals = reactive(new Map<string, Seen>());
  let wanted = 0;
  /** Whether a search was asked for and not yet stopped. */
  let searching = false;
  /** Reports waiting to be shown, by line. */
  let pending: Info[] = [];
  let flushing: ReturnType<typeof setTimeout> | undefined;
  const FLUSH = 150;

  function flush() {
    flushing = undefined;
    if (!pending.length) return;
    const next = [...lines.value];
    pending.forEach((info, i) => info && (next[i] = info));
    pending = [];
    lines.value = next;
  }
  function drop() {
    clearTimeout(flushing);
    flushing = undefined;
    pending = [];
    lines.value = [];
  }

  function remember(fen: string, info: Info) {
    const turn = fen.split(" ")[1] === "b" ? "black" : "white";
    const known = evals.get(fen);
    if (known && known.depth > info.depth) return;
    // Most reports repeat the figure: leave the graph and the bar alone then.
    if (known && known.pawns === whitePawns(info.score, turn) && known.text === whiteScore(info.score, turn)) {
      known.depth = info.depth;
      return;
    }
    evals.set(fen, { pawns: whitePawns(info.score, turn), text: whiteScore(info.score, turn), depth: info.depth });
  }

  useRunnerEvent("analysis.info", (data) => {
    const { session: s, info } = data as { session: number; info: Info };
    if (s !== session.value) return;
    pending[info.multipv - 1] = info;
    if (info.multipv === 1) remember(input.position.value.fen, info);
    // Reports come many times a second; the lines only need to change a few times.
    if (flushing === undefined) flushing = setTimeout(flush, FLUSH);
  });
  useRunnerEvent("analysis.stopped", (data) => {
    if ((data as { session: number }).session === session.value) session.value = null;
  });

  async function begin() {
    const ticket = ++wanted;
    error.value = "";
    searching = true;
    try {
      // Half of the machine's threads, so the runner can still do its own work.
      const threads = Math.max(1, Math.floor((status.value.data?.cpu.threads ?? 2) / 2));
      const live = await unwrap(
        api.POST("/api/analysis", {
          body: {
            build: Number(input.build.value),
            fen: input.start.value,
            moves: input.position.value.moves,
            multipv: Number(input.multipv.value),
            threads,
            hash: 256,
            limits: {},
          },
        }),
      );
      // A newer request may have started while this one was on its way.
      if (ticket === wanted) session.value = live.session;
      else if (!searching) await api.DELETE("/api/analysis");
    } catch (e) {
      if (ticket === wanted) error.value = (e as Error).message;
    }
  }

  /** Ends the search, if one is running (or on its way: the runner then ignores the extra stop). */
  async function stop() {
    wanted++;
    drop();
    const running = session.value !== null || searching;
    session.value = null;
    searching = false;
    if (running) await api.DELETE("/api/analysis");
  }

  // Any change to what is analysed restarts the search, a moment later so stepping through moves does not flood it.
  let timer: ReturnType<typeof setTimeout> | undefined;
  watch(
    [() => input.position.value.fen, input.build, input.multipv, input.active],
    () => {
      clearTimeout(timer);
      if (!input.active.value || !input.build.value) {
        void stop();
        return;
      }
      // What the old search reports is about another position now.
      session.value = null;
      drop();
      timer = setTimeout(begin, 120);
    },
    { immediate: true },
  );
  onBeforeUnmount(() => {
    clearTimeout(timer);
    drop();
    void stop();
  });

  return { lines, error, evals };
}
