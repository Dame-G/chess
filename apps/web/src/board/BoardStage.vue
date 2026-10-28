<script setup lang="ts">
import { computed, ref, watch } from "vue";
import type { DrawShape } from "@lichess-org/chessground/draw";
import type { Key } from "@lichess-org/chessground/types";
import { useToast } from "@eos-ui/vue";
import type { Side } from "../api/types";
import { isLevelSeat, other, pawns, scoreText, seatName, sideResult, winChance } from "../format";
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

/** The eval bar's figure and the side it favours, for the strips to carry where the bar is a thin line. */
const figure = ref<{ text: string; leader: Side } | null>(null);

function strip(side: Side) {
  const seat = s.value![side];
  const you = s.value!.you === side;
  return {
    // A level's name carries its rating ("Dameless 1650"), so the strip does not repeat it.
    seat: isLevelSeat(seat) ? { ...seat, rating: null } : seat,
    name: you ? "You" : seatName(seat),
    ...clockOf(side),
    since: games.clocksAt,
    score: evalShown.value && figure.value?.leader === side ? figure.value.text : null,
    scoreSide: side,
    active: live.value && board.atEnd && board.position.turn === side,
    result: s.value!.status === "finished" ? sideResult(s.value!.outcome, side) : null,
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
const graphShown = computed(() => analysis.value && !!g.value && !board.exploring && board.plies.length > 1);

// The sounds of a game as it is played: each new move at the end, and a game starting.
watch(
  () => g.value?.moves.length ?? 0,
  (n, before) => {
    if (!live.value || before === undefined || n <= before || !board.following) return;
    playMoveSound(soundOf(g.value?.moves[n - 1]?.san));
  },
);
</script>

<template>
  <section
    class="stage"
    :class="{ 'stage--strips': !!s, 'stage--noeval': !evalShown, 'stage--nograph': !graphShown }"
    aria-label="Board"
  >
    <div class="col">
      <div class="rig">
        <Strip v-if="s" v-bind="strip(top)" class="strip-top" />
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
        <EvalBar
          v-if="evalShown"
          :pawns="board.shownEval?.pawns"
          :text="board.shownEval?.text"
          :orientation="board.orientation"
          class="rig-eval"
          @figure="figure = $event"
        />
        <Strip v-if="s" v-bind="strip(bottom)" class="strip-bottom" />
      </div>
      <div class="foot">
        <EvalGraph
          v-if="graphShown"
          label="The evaluation through the game. Click or drag to go to a move."
          :values="graph.values"
          :marks="graph.marks"
          :tips="graph.tips"
          :cursor="board.ply"
          class="graph"
          @select="board.go"
        />
        <Transport v-if="!drilling" />
      </div>
    </div>
  </section>
</template>

<style scoped>
/* The board drives the layout and fills what it is given, always square, with no fixed size anywhere.
   Side by side (75rem and up) its side is the largest square the stage allows, less the strips, the foot and the
   padding, so it is limited by the width and by the height and never overflows the window; the stage is the size
   container, so only the panel and the rail absorb slack. Stacked (below 75rem) it is the width of the column. */
.stage {
  --col: 1.75rem;
  --egap: var(--e-gap-snug);
  --pad: var(--e-gap-base);
  --strip: 2.25rem;
  --strips: 0px;
  --graph: 3.5rem;
  --foot: calc(var(--graph) + 2.75rem + var(--e-gap-tight) + var(--e-gap-snug));
  --side: max(
    10rem,
    min(100cqh - var(--foot) - var(--strips) - 2 * var(--pad), 100cqw - var(--col) - var(--egap) - 2 * var(--pad))
  );

  display: grid;
  place-content: center;
  min-width: 0;
  min-height: 0;
  padding: var(--pad);
  box-sizing: border-box;
  container-type: size;
}

.stage--strips {
  --strips: calc(2 * (var(--strip) + var(--e-gap-tight)));
}

.stage--noeval {
  --col: 0px;
  --egap: 0px;
}

.stage--nograph {
  --graph: 0px;
}

.col {
  display: grid;
  gap: var(--e-gap-snug);
  width: calc(var(--col) + var(--egap) + var(--side));
}

.rig {
  display: grid;
  grid-template:
    "top top" auto
    "eval board" var(--side)
    "bottom bottom" auto / var(--col) var(--side);
  gap: var(--e-gap-tight) var(--egap);
}

.strip-top {
  grid-area: top;
  height: var(--strip);
  margin-inline-start: calc(var(--col) + var(--egap));
}

.rig-eval {
  grid-area: eval;
}

.rig-board {
  grid-area: board;
}

.strip-bottom {
  grid-area: bottom;
  height: var(--strip);
  margin-inline-start: calc(var(--col) + var(--egap));
}

.foot {
  display: grid;
  gap: var(--e-gap-tight);
  margin-inline-start: calc(var(--col) + var(--egap));
}

.graph {
  height: var(--graph);
}

/* Stacked: the stage is as wide as the window and as tall as the board needs. The board is the column's width; from
   40rem up the column is held to what the window's height leaves after the header, the strips and the foot (so the
   board is whole on a short screen) and sits inside the gutter. */
@media (width < 75rem) {
  .stage {
    --chrome: 5.5rem;
    --bar: 0.4375rem;
    --cap: max(100dvh - var(--chrome) - var(--foot) - var(--strips) - var(--bar), 20rem);
    --col: 0px;
    --egap: 0px;

    display: block;
    padding: var(--e-gutter);
    container-type: inline-size;
  }

  .col {
    width: min(100%, var(--cap));
    margin-inline: auto;
  }

  .rig {
    grid-template:
      "top" auto
      "board" auto
      "eval" auto
      "bottom" auto / minmax(0, 1fr);
    gap: 0;
  }

  .rig-board {
    width: 100%;
  }

  /* The eval bar lies flush under the board: its corners are square where they meet. */
  .stage:not(.stage--noeval) .rig-board {
    border-end-start-radius: 0;
    border-end-end-radius: 0;
  }

  .strip-top,
  .strip-bottom {
    margin-inline-start: 0;
  }

  .foot {
    margin-inline-start: 0;
  }
}

/* A tall window has height to spare for the board: it takes the whole column. */
@media (width < 75rem) and (orientation: portrait) {
  .stage {
    --cap: 100%;
  }
}

/* Phones: the board runs edge to edge, with no padding or gutter at either side. The strips, the graph and the
   controls keep their own gutter. */
@media (width < 40rem) {
  .stage {
    padding-inline: 0;
    padding-block-start: 0;
  }

  .col {
    width: 100%;
  }

  .rig-board {
    border-radius: 0;
    box-shadow: none;
  }

  .strip-top,
  .strip-bottom,
  .foot {
    padding-inline: var(--e-gutter);
  }
}
</style>
