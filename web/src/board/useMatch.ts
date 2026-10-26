import { onBeforeUnmount, ref, shallowRef, watch, type Ref } from "vue";
import { useRunnerEvent } from "../api/events";
import { api } from "../api/runner";
import { unwrap } from "../api/queries";
import type { components } from "../api/schema";
import type { Match, Side } from "./matchText";

type Score = components["schemas"]["Score"];
type MatchMove = components["schemas"]["MatchMove"];

/** What an engine in a match last reported while thinking, with the position it was thinking about. */
export interface Thinking {
  /** The move it was choosing, 1 for the first move of the game. */
  ply: number;
  /** The position, as a FEN or `startpos`, so its line can be written in SAN. */
  fen: string;
  depth: number;
  seldepth: number;
  score: Score;
  nodes: number;
  nps: number;
  pv: string[];
}

/**
 * Both clocks as of `at` (a `performance.now()` reading): the side to move runs down from there, the other holds.
 * The strips count down locally between the server's updates and are corrected by each move.
 */
export interface ClockModel {
  white: number | null;
  black: number | null;
  turn: Side;
  at: number;
  ticking: boolean;
}

interface InfoEvent {
  id: number;
  ply: number;
  side: Side;
  depth: number;
  seldepth: number;
  score: Score;
  nodes: number;
  nps: number;
  pv: string[];
}
type MoveEvent = MatchMove & { id: number };
interface Queued {
  kind: "move" | "info" | "ended";
  data: unknown;
}

/** A match from the runner, kept current by its events: moves, the thinking engines, and the clocks. */
export function useMatch(id: Ref<number>) {
  const match = shallowRef<Match>();
  const failed = ref(false);
  const clock = shallowRef<ClockModel>();
  const thinking = shallowRef<Record<Side, Thinking | undefined>>({ white: undefined, black: undefined });
  /** Counts takebacks, which shorten the game: the board watches it to follow the line back. */
  const takebacks = ref(0);
  /** In a game of yours on Lichess: whether the opponent has left, and when the win can be claimed (as of `at`). */
  const gone = shallowRef<{ gone: boolean; claimIn: number | null; at: number }>();

  let queue: Queued[] | undefined = [];
  let wanted = 0;

  function clocks(m: Match): ClockModel {
    const hasClock = m.time.type === "clock";
    return {
      white: hasClock ? (m.clock_white_ms ?? null) : null,
      black: hasClock ? (m.clock_black_ms ?? null) : null,
      turn: m.turn,
      at: performance.now(),
      ticking: hasClock && m.status === "running",
    };
  }

  async function load(quiet = false) {
    const ticket = ++wanted;
    const mine = id.value;
    if (!mine) return;
    if (!quiet) {
      queue = [];
      failed.value = false;
    }
    try {
      const m = await unwrap(api.GET("/api/matches/{id}", { params: { path: { id: mine } } }));
      if (ticket !== wanted) return;
      match.value = m;
      clock.value = clocks(m);
      if (!quiet) thinking.value = { white: undefined, black: undefined };
      const pending = queue ?? [];
      queue = undefined;
      for (const q of pending) handle(q);
    } catch {
      if (ticket !== wanted) return;
      // Nothing to apply the waiting events to: let them go rather than pile up.
      queue = undefined;
      if (!quiet) failed.value = true;
    }
  }

  function handle(q: Queued) {
    const m = match.value;
    if (!m || m.id !== id.value) return;
    if (q.kind === "move") {
      const mv = q.data as MoveEvent;
      if (mv.ply <= m.moves.length) return;
      if (mv.ply > m.moves.length + 1) {
        void load(true);
        return;
      }
      const next = mv.side === "white" ? "black" : "white";
      match.value = {
        ...m,
        moves: [...m.moves, mv],
        turn: next,
        clock_white_ms: mv.clock_white_ms ?? m.clock_white_ms,
        clock_black_ms: mv.clock_black_ms ?? m.clock_black_ms,
      };
      clock.value = clocks(match.value);
    } else if (q.kind === "info") {
      const info = q.data as InfoEvent;
      // Reports of an earlier move (replayed by the stream) say nothing about this one.
      if (info.ply !== m.moves.length + 1 || m.status !== "running") return;
      const fen = m.moves.at(-1)?.fen_after ?? m.start_fen;
      thinking.value = {
        ...thinking.value,
        [info.side]: {
          ply: info.ply,
          fen,
          depth: info.depth,
          seldepth: info.seldepth,
          score: info.score,
          nodes: info.nodes,
          nps: info.nps,
          pv: info.pv,
        },
      };
    } else {
      // The runner holds the record of how it ended.
      void load(true);
    }
  }

  function take(kind: Queued["kind"], data: unknown) {
    if ((data as { id?: number }).id !== id.value) return;
    const q = { kind, data };
    if (queue) queue.push(q);
    else handle(q);
  }
  useRunnerEvent("match.move", (d) => take("move", d));
  useRunnerEvent("match.info", (d) => take("info", d));
  useRunnerEvent("match.ended", (d) => take("ended", d));
  useRunnerEvent("match.takeback", (d) => {
    if ((d as { id?: number }).id === id.value) void load(true).then(() => takebacks.value++);
  });

  // Offers change in a Lichess game: the record carries them, and the panel answers them.
  useRunnerEvent("match.offer", (d) => {
    const o = d as { id?: number; draw_offer?: Side | null; takeback_offer?: Side | null };
    const m = match.value;
    if (o.id !== id.value || !m?.lichess) return;
    match.value = {
      ...m,
      lichess: { ...m.lichess, draw_offer: o.draw_offer ?? null, takeback_offer: o.takeback_offer ?? null },
    };
  });
  useRunnerEvent("match.opponent_gone", (d) => {
    const g = d as { id?: number; gone?: boolean; claim_win_in_seconds?: number | null };
    if (g.id !== id.value) return;
    gone.value = { gone: !!g.gone, claimIn: g.claim_win_in_seconds ?? null, at: performance.now() };
  });

  watch(
    id,
    () => {
      gone.value = undefined;
      void load();
    },
    { immediate: true },
  );

  // A runner that restarted or a stream that dropped cannot say what it missed: check on a live match now and then.
  const watchdog = setInterval(() => {
    if (match.value?.status === "running" && match.value.id === id.value) void load(true);
  }, 8000);
  onBeforeUnmount(() => {
    clearInterval(watchdog);
    wanted++;
  });

  return { match, failed, clock, thinking, takebacks, gone };
}
