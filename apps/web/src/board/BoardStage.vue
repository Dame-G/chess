<script setup lang="ts">
import { computed, ref, watch } from "vue";
import type { DrawShape } from "@lichess-org/chessground/draw";
import type { Key } from "@lichess-org/chessground/types";
import { useToast } from "@eos-ui/vue";
import type { Side } from "../api/types";
import { other, pawns, scoreText, winChance } from "../format";
import { sideRef, type PlayerRef } from "../ui/player";
import { answerArrow } from "./drill";
import { useDrills } from "../stores/drills";
import { useGames } from "../stores/games";
import { usePolicy } from "../stores/policy";
import { useEvaluation } from "../stores/evaluation";
import Board from "./Board.vue";
import EvalBar from "./EvalBar.vue";
import EvalGraph from "./EvalGraph.vue";
import Strip from "./Strip.vue";
import Transport from "./Transport.vue";
import { useBoard } from "./board";
import { materialLead } from "./material";
import { isLive, onLichessAsYou } from "./modes";
import { hasBetter, markOf } from "./review";
import { playMoveSound, soundOf } from "./sound";

/** The board and what sits around it: the seats' strips, the eval bar, and under it the graph and the transport. */
const board = useBoard();
const games = useGames();
const policy = usePolicy();
const evaluation = useEvaluation();
const drills = useDrills();
const toast = useToast();

/** A drill takes the board: the game or position under it shows neither seats nor graph. */
const drilling = computed(() => board.drilling);
const g = computed(() => (drilling.value ? null : board.game));
const s = computed(() => (drilling.value ? null : board.summary));
const live = computed(() => !!s.value && isLive(s.value));
const playing = computed(() => board.mode === "play");

/** Analysis on screen: never while You play (on Lichess nothing else is on screen), never against the policy. */
const analysis = computed(() => policy.allowed && !playing.value && !drilling.value);
const evalShown = computed(() => analysis.value && !(s.value && onLichessAsYou(s.value) && live.value));

const bottom = computed<Side>(() => board.orientation);
const top = computed<Side>(() => other(board.orientation));

/** A side's clock: what it holds now in a live game at its newest move, else what it had after its last move shown. */
function clockOf(side: Side): { ms: number | null; running: boolean } {
  const game = g.value;
  if (!game) return { ms: null, running: false };
  const tc = game.summary.time;
  if (live.value && board.atEnd && game.clocks && !board.exploring) {
    const turn = board.position.turn;
    return {
      ms: game.clocks[side],
      running: turn === side && game.summary.status === "live" && game.moves.length >= 2,
    };
  }
  if (tc.type !== "clock") return { ms: null, running: false };
  const upto = board.exploring ? (board.tree.isMain(board.node) ? board.node.ply : 0) : board.ply;
  const moves = game.moves.slice(0, upto);
  const first = board.plies[0]?.turn ?? "white";
  for (let i = moves.length - 1; i >= 0; i--) {
    const mover: Side = (i % 2 === 0) === (first === "white") ? "white" : "black";
    const ms = moves[i]!.clock_ms;
    if (mover === side && ms != null) return { ms, running: false };
  }
  return { ms: tc.initial_ms, running: false };
}

/** Who sits at a side: the seat in a game (you are "You"), else just the colour (a free position, a drill). */
function player(side: Side): PlayerRef {
  const sum = s.value;
  if (!sum) return { name: side === "white" ? "White" : "Black" };
  return sideRef(sum, side);
}

/** A position with a move waiting in it: the side to move is told so, where no running clock tells it. */
const turnShown = computed(() => drilling.value || board.exploring || (live.value && board.atEnd));
const material = computed(() => materialLead(board.position.fen));

function strip(side: Side) {
  return {
    player: player(side),
    side,
    ...clockOf(side),
    since: games.clocksAt,
    toMove: turnShown.value && board.position.turn === side,
    mine: live.value && s.value?.you === side,
    material: side === "white" ? material.value : -material.value,
    // The engine's score rides at the end of the bottom strip: the bar is too thin to print it on.
    score: side === bottom.value && evalShown.value ? board.shownScore : null,
  };
}

/** Whether You may move on the board now: your turn in a game where moving is allowed, or anywhere in Explore. */
const yourTurn = computed(
  () => playing.value && !!g.value?.actions.includes("move") && board.atEnd && board.position.turn === s.value?.you,
);
const drillMovable = computed(() => drilling.value && !!drills.active && !drills.result && !drills.answering);
const movable = computed(() => board.exploring || yourTurn.value || drillMovable.value);
const premove = computed(() => (playing.value && board.atEnd ? (s.value?.you ?? undefined) : undefined));

const boardRef = ref<InstanceType<typeof Board>>();
async function onMove(uci: string) {
  if (drilling.value) {
    try {
      const r = await drills.answer(uci);
      // A move that failed the drill goes back: the board shows the position and the answer, not the try.
      if (r?.outcome === "fail") boardRef.value?.reset();
    } catch (e) {
      boardRef.value?.reset();
      toast.show({
        title: "That drill did not go through",
        message: e instanceof Error ? e.message : String(e),
        tone: "warning",
      });
    }
    return;
  }
  if (board.exploring) {
    board.exploreMove(uci);
    return;
  }
  try {
    await games.act("move", { uci });
  } catch (e) {
    boardRef.value?.reset();
    toast.show({
      title: "That move did not go through",
      message: e instanceof Error ? e.message : String(e),
      tone: "warning",
    });
  }
}

const review = computed(() => (board.exploring ? null : games.review));
const reviewed = computed(() => (review.value && board.ply > 0 ? review.value.plies[board.ply - 1] : undefined));

const arrow = (uci: string, brush: string): DrawShape => ({
  orig: uci.slice(0, 2) as Key,
  dest: uci.slice(2, 4) as Key,
  brush,
});
const shapes = computed<DrawShape[]>(() => {
  if (drilling.value) {
    const best = drills.result ? answerArrow(drills.result) : null;
    return best ? [arrow(best, "green")] : [];
  }
  if (!analysis.value) return [];
  if (board.exploring) {
    const info = evaluation.info;
    const line = board.tree.line(board.node);
    const t = evaluation.target;
    const pv = info?.lines[0]?.pv[0];
    if (!pv || !t || t.moves.join() !== line.moves.join()) return [];
    return [arrow(pv, "green")];
  }
  const r = reviewed.value;
  if (r && hasBetter(r)) return [arrow(r.best_uci, "review")];
  const t = games.thinking;
  if (board.mode === "watch" && board.atEnd && t?.pv[0]) {
    return [arrow(t.pv[0], t.side === "white" ? "engineWhite" : "engineBlack")];
  }
  return [];
});

/** The graph: White's share of a win at each position, from the review where there is one, else the engines' scores. */
const graph = computed(() => {
  const n = board.plies.length;
  const values: (number | null)[] = Array.from({ length: n }, () => null);
  const marks: (string | null)[] = Array.from({ length: n }, () => null);
  const scores: (string | null)[] = Array.from({ length: n }, () => null);
  const r = review.value;
  const share = (p: number) => 2 * winChance(p) - 1;
  if (r?.plies.length) {
    values[0] = share(pawns(r.plies[0]!.eval_before));
    scores[0] = scoreText(r.plies[0]!.eval_before);
    for (const p of r.plies) {
      const turn = p.side === "white" ? "black" : "white";
      values[p.ply] = share(pawns(p.eval_after, turn));
      scores[p.ply] = scoreText(p.eval_after, turn);
      const tone = markOf(p.classification).tone;
      marks[p.ply] = tone && tone !== "great" ? `--rv-${tone}` : null;
    }
  } else {
    values[0] = 0;
    g.value?.moves.forEach((m, i) => {
      if (m.eval) {
        values[i + 1] = share(pawns(m.eval));
        scores[i + 1] = scoreText(m.eval);
      }
    });
  }
  // What the pointer reads out over each position: the move, then the score where there is one.
  const tips = values.map((_, i) => {
    const san = board.plies[i]?.san;
    const move = i === 0 || !san ? "Start" : `${Math.ceil(i / 2)}${i % 2 ? "." : "…"} ${san}`;
    return scores[i] ? `${move}  ${scores[i]}` : move;
  });
  return { values, marks, tips };
});
/** A graph with nothing to draw (a game no engine has looked at) is the baseline, not an empty frame. */
const graphShown = computed(
  () =>
    analysis.value &&
    !!g.value &&
    !board.exploring &&
    board.plies.length > 1 &&
    graph.value.values.some((v, i) => i > 0 && v !== null),
);

// The sounds of a game as it is played: each new move at the end, and a game starting.
watch(
  () => g.value?.moves.length ?? 0,
  (n, before) => {
    if (!live.value || before === undefined || n <= before || !board.following) return;
    playMoveSound(soundOf(g.value?.moves[n - 1]?.san));
  },
);
// A game you are looking at ending says so: a soft chime once, not when a finished game is opened.
watch(
  () => [s.value?.id, s.value?.status] as const,
  ([id, status], [was, before]) => {
    if (id === was && before && isLive({ status: before }) && status && !isLive({ status }) && board.following) {
      playMoveSound("end");
    }
  },
);
</script>

<template>
  <section class="stage" aria-label="Board">
    <div class="col">
      <Strip :class="'strip-top'" v-bind="strip(top)" />
      <div class="eval-slot">
        <EvalBar
          v-if="evalShown"
          :score="board.shownScore?.score"
          :turn="board.shownScore?.turn"
          :orientation="board.orientation"
        />
      </div>
      <Board
        ref="boardRef"
        :position="board.position"
        :orientation="board.orientation"
        :movable="movable"
        :premove="premove"
        :shapes="shapes"
        class="rig-board"
        @move="onMove"
      />
      <Strip class="strip-bottom" v-bind="strip(bottom)" />
      <div class="graph-slot">
        <EvalGraph
          v-if="graphShown"
          label="The evaluation through the game. Click or drag to go to a move."
          :values="graph.values"
          :marks="graph.marks"
          :tips="graph.tips"
          :cursor="board.ply"
          @select="board.go"
        />
        <div v-else class="graph-rest" aria-hidden="true"></div>
      </div>
      <Transport class="transport" :flip-only="drilling" />
    </div>
  </section>
</template>

<style scoped>
/* The board never changes size or place: the strips, the graph and the transport keep their heights whether or not
   they have anything to show, and the eval bar keeps its column. Its side is the largest square the stage allows, less
   that chrome and the padding; the stage is the size container, so only the panel and the rail absorb slack. */
.stage {
  --col: 1rem;
  --egap: var(--sp-2);
  --pad: var(--sp-3);
  --gap: var(--sp-1);
  --chrome: calc(2 * var(--strip) + var(--graph) + var(--transport) + 4 * var(--gap));

  /* A container query unit is the container's content box, so the padding is already out of it. */
  --side: max(10rem, min(100cqh - var(--chrome), 100cqw - var(--col) - var(--egap)));

  display: grid;
  place-content: center;
  min-width: 0;
  min-height: 0;
  padding: var(--pad);
  box-sizing: border-box;
  container-type: size;
}

.col {
  display: grid;
  grid-template:
    "top top" var(--strip)
    "eval board" var(--side)
    "bottom bottom" var(--strip)
    "graph graph" var(--graph)
    "transport transport" var(--transport) / var(--col) var(--side);
  gap: var(--gap) var(--egap);
}

.strip-top {
  grid-area: top;
  margin-inline-start: calc(var(--col) + var(--egap));
}

.eval-slot {
  grid-area: eval;
  min-height: 0;
}

.rig-board {
  grid-area: board;
}

.strip-bottom {
  grid-area: bottom;
  margin-inline-start: calc(var(--col) + var(--egap));
}

.graph-slot {
  grid-area: graph;
  margin-inline-start: calc(var(--col) + var(--egap));
  min-width: 0;
}

/* The slot with nothing to draw: the graph's baseline, so the board does not move when a graph arrives. */
.graph-rest {
  position: relative;
  height: 100%;
}

.graph-rest::after {
  content: "";
  position: absolute;
  inset-block-start: 50%;
  inset-inline: 0;
  height: var(--e-stroke-hair);
  background: var(--c-line);
}

.transport {
  grid-area: transport;
  margin-inline-start: calc(var(--col) + var(--egap));
}

/* A tablet (48rem to 75rem) is taller than it is wide: the board sits at the top of its column, under the header, where
   the eye starts, instead of floating in the middle of the height. */
@media (width >= 48rem) and (width < 75rem) {
  .stage {
    place-content: start center;
  }
}

/* Below 48rem (a phone) the board runs edge to edge: the column is the window's width, with no eval bar and no
   padding at either side, and the strips and the controls keep a gutter of their own. The page scrolls. */
@media (width < 48rem) {
  .stage {
    --col: 0px;
    --egap: 0px;
    --pad: 0px;
    --side: 100cqw;

    display: block;
    container-type: inline-size;
  }

  .col {
    grid-template:
      "top" var(--strip)
      "board" var(--side)
      "bottom" var(--strip)
      "graph" var(--graph)
      "transport" var(--transport) / minmax(0, 1fr);
  }

  .eval-slot {
    display: none;
  }

  .rig-board {
    border-radius: 0;
    box-shadow: none;
  }

  .strip-top,
  .strip-bottom,
  .graph-slot,
  .transport {
    margin-inline: var(--sp-3);
  }
}
</style>
