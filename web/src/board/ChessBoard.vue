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
}>();
const emit = defineEmits<{ move: [uci: string] }>();

const el = ref<HTMLElement>();
let cg: Api | undefined;

function config() {
  const p = props.position;
  return {
    fen: p.fen,
    turnColor: p.turn,
    check: p.check ? p.turn : false,
    lastMove: p.lastMove,
    orientation: props.orientation ?? "white",
    movable: {
      free: false,
      color: props.movable ? p.turn : undefined,
      dests: props.movable ? p.dests : new Map(),
      showDests: true,
    },
    drawable: { autoShapes: props.shapes ?? [] },
  } as const;
}

onMounted(() => {
  cg = Chessground(el.value!, {
    ...config(),
    animation: { duration: 150 },
    premovable: { enabled: false },
    movable: {
      ...config().movable,
      events: { after: (from: Key, to: Key) => emit("move", boardMove(props.position, from, to)) },
    },
  });
});

watch(
  () => [props.position, props.orientation, props.movable],
  () => cg?.set(config()),
);
watch(
  () => props.shapes,
  (shapes) => cg?.setAutoShapes(shapes ?? []),
);

onBeforeUnmount(() => cg?.destroy());
</script>

<template>
  <div class="board">
    <div ref="el" class="board-cg"></div>
  </div>
</template>
