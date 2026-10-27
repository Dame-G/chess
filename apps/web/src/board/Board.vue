<script setup lang="ts">
import { computed, nextTick, onBeforeUnmount, onMounted, ref, watch } from "vue";
import { Chessground } from "@lichess-org/chessground";
import type { Api } from "@lichess-org/chessground/api";
import type { DrawShape } from "@lichess-org/chessground/draw";
import type { Color, Key } from "@lichess-org/chessground/types";
import "@lichess-org/chessground/assets/chessground.base.css";
import "@lichess-org/chessground/assets/chessground.cburnett.css";
import "./board.css";
import { boardMove, promotingColor, type Position } from "./position";
import { promotionCells } from "./promotion";

const props = defineProps<{
  position: Position;
  orientation?: Color;
  /** Whether the side to move can move on the board. */
  movable?: boolean;
  /** Arrows, such as the engine's best moves. */
  shapes?: DrawShape[];
  /** The side that may premove while the other is to move: a move set down early is played the moment it is legal. */
  premove?: Color;
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
  const mine = live ? props.premove : undefined;
  return {
    fen: p.fen,
    turnColor: p.turn,
    check: p.check ? p.turn : false,
    // An empty list clears the highlight; leaving it out would keep the last one.
    lastMove: p.lastMove ?? [],
    orientation: props.orientation ?? "white",
    viewOnly: !!props.still,
    ...(animation ? { animation } : {}),
    movable: {
      free: false,
      color: live ? (mine ?? p.turn) : undefined,
      dests: live && (!mine || mine === p.turn) ? p.dests : new Map(),
      showDests: true,
    },
    premovable: { enabled: !!mine, showDests: true },
    drawable: { autoShapes: props.shapes ?? [] },
  } as const;
}

/** A pawn that has reached the last rank waits here for the piece it becomes: the move is sent once one is chosen. */
const pending = ref<{ from: Key; to: Key; color: Color } | null>(null);
const chooser = ref<HTMLElement>();
const cells = computed(() => (pending.value ? promotionCells(pending.value.to, props.orientation ?? "white") : []));

function moved(from: Key, to: Key) {
  const color = promotingColor(props.position, from, to);
  if (!color) return emit("move", boardMove(props.position, from, to));
  pending.value = { from, to, color };
  void nextTick(() => chooser.value?.querySelector<HTMLElement>("button")?.focus());
}
function choose(letter: string) {
  const p = pending.value;
  if (!p) return;
  pending.value = null;
  emit("move", boardMove(props.position, p.from, p.to, letter));
}
/** Escape or a click outside: the pawn goes back where it was. */
function cancel() {
  if (!pending.value) return;
  pending.value = null;
  cg?.cancelPremove();
  cg?.set(config({ enabled: false }));
}
function onKey(e: KeyboardEvent) {
  if (e.key !== "Escape" || !pending.value) return;
  e.stopPropagation();
  e.preventDefault();
  cancel();
}
onMounted(() => window.addEventListener("keydown", onKey, true));
onBeforeUnmount(() => window.removeEventListener("keydown", onKey, true));

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
        review: { key: "rv", color: "#148275", opacity: 0.9, lineWidth: 12 },
        engineWhite: { key: "ew", color: "#c8772e", opacity: 0.85, lineWidth: 10 },
        engineBlack: { key: "eb", color: "#2a2420", opacity: 0.8, lineWidth: 10 },
      },
    },
    movable: {
      ...c.movable,
      events: { after: moved },
    },
  });
  lastFen = props.position.fen;
  lastSet = performance.now();
});

watch(
  () => props.position,
  (p) => {
    pending.value = null;
    cg?.set(config(motion(p)));
    // A premove waiting for this turn is played now, if it is still legal.
    if (props.premove && props.premove === p.turn) cg?.playPremove();
  },
);
watch(
  () => [props.orientation, props.movable, props.still, props.premove],
  () => {
    pending.value = null;
    cg?.set(config({ enabled: false }));
  },
);
watch(
  () => props.shapes,
  (shapes) => cg?.setAutoShapes(shapes ?? []),
);

onBeforeUnmount(() => cg?.destroy());

defineExpose({
  /** Puts the pieces back where the position says, such as after a move the app refused. */
  reset: () => {
    cg?.cancelPremove();
    cg?.set(config({ enabled: false }));
  },
});
</script>

<template>
  <div class="board" :class="{ 'board--still': still }">
    <div ref="el" class="board-cg"></div>
    <div v-if="pending" ref="chooser" class="cg-wrap promo" role="dialog" aria-label="Promote to">
      <button type="button" class="promo-dim" tabindex="-1" aria-label="Cancel" @click="cancel"></button>
      <button
        v-for="c in cells"
        :key="c.choice.letter"
        type="button"
        class="promo-choice"
        :style="{ left: `${c.col * 12.5}%`, top: `${c.row * 12.5}%` }"
        :aria-label="c.choice.label"
        :title="c.choice.label"
        @click="choose(c.choice.letter)"
      >
        <piece :class="[c.choice.role, pending.color]"></piece>
      </button>
    </div>
  </div>
</template>
