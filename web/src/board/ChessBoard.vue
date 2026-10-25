<script setup lang="ts">
import { onBeforeUnmount, onMounted, ref, watch } from "vue";
import { Chessground } from "@lichess-org/chessground";
import type { Api } from "@lichess-org/chessground/api";
import type { DrawShape } from "@lichess-org/chessground/draw";
import type { Color, Key } from "@lichess-org/chessground/types";
import "@lichess-org/chessground/assets/chessground.base.css";
import "@lichess-org/chessground/assets/chessground.cburnett.css";
import "./board.css";
import { boardMove, type Position } from "./position";

const props = defineProps<{
  position: Position;
  orientation?: Color;
  /** Whether the side to move can move on the board. */
  movable?: boolean;
  /** Arrows, such as the engine's best moves. */
  shapes?: DrawShape[];
  /** A board to look at only, such as a preview: no moves, no animation. */
  still?: boolean;
}>();
const emit = defineEmits<{ move: [uci: string] }>();

const el = ref<HTMLElement>();
let cg: Api | undefined;

/** The longest a piece takes to travel, and the shortest worth animating. */
const SLIDE = 180;
const FASTEST = 70;
const reduced = typeof matchMedia === "function" ? matchMedia("(prefers-reduced-motion: reduce)") : undefined;

let lastSet = 0;
let lastFen = props.position.fen;

/** How many squares differ between two positions: a step changes two to four, a jump many more. */
function changed(a: string, b: string): number {
  const expand = (fen: string) =>
    fen
      .split(" ")[0]!
      .replace(/\d/g, (n) => ".".repeat(Number(n)))
      .replace(/\//g, "");
  const x = expand(a);
  const y = expand(b);
  let n = 0;
  for (let i = 0; i < 64; i++) if (x[i] !== y[i]) n++;
  return n;
}

/**
 * Moves slide in as long as the steps are not coming faster than the slide itself: held arrow keys and fast autoplay
 * snap instead of queueing up, a jump across the game snaps, and the slide shortens to fit the pace in between.
 */
function motion(next: Position) {
  const now = performance.now();
  const gap = now - lastSet;
  lastSet = now;
  const previous = lastFen;
  lastFen = next.fen;
  if (props.still || reduced?.matches || previous === next.fen) return { enabled: false };
  if (changed(previous, next.fen) > 4) return { enabled: false };
  const duration = Math.min(SLIDE, gap * 0.85);
  return duration < FASTEST ? { enabled: false } : { enabled: true, duration };
}

function config(animation?: { enabled: boolean; duration?: number }) {
  const p = props.position;
  const live = props.movable && !props.still;
  return {
    fen: p.fen,
    turnColor: p.turn,
    check: p.check ? p.turn : false,
    lastMove: p.lastMove,
    orientation: props.orientation ?? "white",
    viewOnly: !!props.still,
    ...(animation ? { animation } : {}),
    movable: {
      free: false,
      color: live ? p.turn : undefined,
      dests: live ? p.dests : new Map(),
      showDests: true,
    },
    drawable: { autoShapes: props.shapes ?? [] },
  } as const;
}

onMounted(() => {
  const c = config();
  cg = Chessground(el.value!, {
    ...c,
    animation: { enabled: !props.still, duration: SLIDE },
    coordinates: !props.still,
    highlight: { lastMove: true, check: true },
    draggable: { distance: 3, autoDistance: true, showGhost: true },
    selectable: { enabled: true },
    // The two engines of a match, in hues apart from the green of analysis and from the wood.
    drawable: {
      ...c.drawable,
      brushes: {
        green: { key: "g", color: "#15781b", opacity: 1, lineWidth: 10 },
        red: { key: "r", color: "#882020", opacity: 1, lineWidth: 10 },
        blue: { key: "b", color: "#003088", opacity: 1, lineWidth: 10 },
        yellow: { key: "y", color: "#e68f00", opacity: 1, lineWidth: 10 },
        engineWhite: { key: "ew", color: "#3d7fd6", opacity: 0.85, lineWidth: 10 },
        engineBlack: { key: "eb", color: "#d9822b", opacity: 0.85, lineWidth: 10 },
      },
    },
    premovable: { enabled: false },
    movable: {
      ...c.movable,
      events: { after: (from: Key, to: Key) => emit("move", boardMove(props.position, from, to)) },
    },
  });
  lastFen = props.position.fen;
  lastSet = performance.now();
});

watch(
  () => props.position,
  (p) => cg?.set(config(motion(p))),
);
watch(
  () => [props.orientation, props.movable, props.still],
  () => cg?.set(config({ enabled: false })),
);
watch(
  () => props.shapes,
  (shapes) => cg?.setAutoShapes(shapes ?? []),
);

onBeforeUnmount(() => cg?.destroy());
</script>

<template>
  <div class="board" :class="{ 'board--still': still }">
    <div ref="el" class="board-cg"></div>
  </div>
</template>
