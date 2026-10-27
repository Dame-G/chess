<script setup lang="ts">
import { computed, onBeforeUnmount, onMounted, ref, watch } from "vue";
import type { DrawShape } from "@lichess-org/chessground/draw";
import type { Key } from "@lichess-org/chessground/types";
import { useToast } from "@eos-ui/vue";
import type { Side } from "../api/types";
import { isLevelSeat, other, pawns, seatName, sideResult, winChance } from "../format";
import { answerArrow } from "./drill";
import { useDrills } from "../stores/drills";
import { useGames } from "../stores/games";
import { usePolicy } from "../stores/policy";
import { useVersions } from "../stores/versions";
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
const versions = useVersions();
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

function strip(side: Side) {
  const seat = s.value![side];
  const you = s.value!.you === side;
  return {
    // A level's name carries its rating ("Dameless 1650"), so the strip does not repeat it.
    seat: isLevelSeat(seat) ? { ...seat, rating: null } : seat,
    name: you ? "You" : seatName(seat, versions.name(seat.version)),
    ...clockOf(side),
    since: games.clocksAt,
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
  const r = review.value;
  const share = (p: number) => 2 * winChance(p) - 1;
  if (r?.plies.length) {
    values[0] = share(pawns(r.plies[0]!.eval_before));
    for (const p of r.plies) {
      values[p.ply] = share(pawns(p.eval_after, p.side === "white" ? "black" : "white"));
      const tone = markOf(p.classification).tone;
      marks[p.ply] = tone && tone !== "great" ? `--rv-${tone}` : null;
    }
  } else {
    values[0] = 0;
    g.value?.moves.forEach((m, i) => {
      if (m.eval) values[i + 1] = share(pawns(m.eval));
    });
  }
  return { values, marks };
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

// The board's size step: eight device pixels, in CSS pixels. It follows the pixel ratio as the window moves between
// screens or the page is zoomed.
let stepQuery: MediaQueryList | undefined;
function setStep() {
  const ratio = window.devicePixelRatio || 1;
  document.documentElement.style.setProperty("--cg-step", `${8 / ratio}px`);
  stepQuery?.removeEventListener("change", setStep);
  stepQuery = matchMedia(`(resolution: ${ratio}dppx)`);
  stepQuery.addEventListener("change", setStep);
}
onMounted(setStep);
onBeforeUnmount(() => stepQuery?.removeEventListener("change", setStep));
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
        <EvalBar
          v-if="evalShown"
          :pawns="board.shownEval?.pawns"
          :text="board.shownEval?.text"
          :orientation="board.orientation"
          class="rig-eval"
        />
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
        <Strip v-if="s" v-bind="strip(bottom)" class="strip-bottom" />
      </div>
      <div class="foot">
        <EvalGraph
          v-if="graphShown"
          label="The evaluation through the game. Click or drag to go to a move."
          :values="graph.values"
          :marks="graph.marks"
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
/* The board drives the layout. Its side is the largest square the stage allows, less the strips, the foot and the
   padding; the stage is the size container, so only the panel and the rail absorb slack. */
.stage {
  --col: 2rem;
  --egap: var(--e-gap-snug);
  --pad: var(--e-gap-base);
  --strip: 2.25rem;
  --strips: 0px;
  --graph: 3.5rem;
  --foot: calc(var(--graph) + 2.75rem + var(--e-gap-tight) + var(--e-gap-snug));
  --side-raw: min(
    100cqh - var(--foot) - var(--strips) - 2 * var(--pad),
    100cqw - var(--col) - var(--egap) - 2 * var(--pad)
  );
  --side: var(--side-raw);

  display: grid;
  place-content: center;
  min-width: 0;
  min-height: 0;
  padding: var(--pad);
  box-sizing: border-box;
  container-type: size;
}

/* chessground draws its squares on a whole number of device pixels, eight squares wide, and floors the rest: a board
   that is not a multiple of eight device pixels gets a sliver of empty frame on its right and bottom. The side is
   rounded down to that step (`--cg-step`, eight device pixels in CSS pixels, set from the screen's pixel ratio), so
   the squares meet the frame exactly. */
@supports (width: round(down, 10px, 1px)) {
  .stage {
    --side: round(down, var(--side-raw), var(--cg-step, 8px));
  }
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
  grid-template-columns: var(--col) var(--side);
  grid-template-rows: auto var(--side) auto;
  gap: var(--e-gap-tight) var(--egap);
}

.strip-top {
  grid-area: 1 / 2;
  height: var(--strip);
}

.rig-eval {
  grid-area: 2 / 1;
}

.rig-board {
  grid-area: 2 / 2;
}

.strip-bottom {
  grid-area: 3 / 2;
  height: var(--strip);
}

.foot {
  display: grid;
  gap: var(--e-gap-tight);
  margin-inline-start: calc(var(--col) + var(--egap));
}

.graph {
  height: var(--graph);
}

/* Stacked: the stage is as wide as the window and as tall as the board needs; the board leaves room for the header,
   the foot and a glimpse of the panel. */
@media (width < 75rem) {
  .stage {
    --chrome: 9rem;
    --side-raw: min(
      100cqw - var(--col) - var(--egap) - 2 * var(--pad),
      max(100dvh - var(--chrome) - var(--foot) - var(--strips), 16rem)
    );

    container-type: inline-size;
  }
}

@media (width < 40rem) {
  .stage {
    --pad: 0.5rem;
    --col: 1.5rem;
  }

  .stage--noeval {
    --col: 0px;
  }
}
</style>
