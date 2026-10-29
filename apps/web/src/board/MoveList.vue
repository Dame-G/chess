<script setup lang="ts">
import { computed, ref } from "vue";
import { EVirtualList } from "@eos-ui/vue/components/EVirtualList";
import UiEmpty from "../ui/UiEmpty.vue";
import { useMedia } from "../media";
import { focusCurrent, followCurrent, stepOf, type Step } from "./follow";
import MoveRow from "./MoveRow.vue";
import type { Classification } from "../api/types";

const props = defineProps<{
  /** The moves in SAN, in order. */
  sans: string[];
  /** How many moves are on the board. */
  cursor: number;
  /** The FEN the line starts from: it sets the first move number and who moves first. */
  start: string;
  /** A game is on its way: placeholders instead of "no moves". */
  loading?: boolean;
  /** The review's judgement of each move and the line to show for it, by ply (index 0 is ply 1). */
  marks?: (Classification | null)[];
  hints?: string[];
}>();
const emit = defineEmits<{ go: [ply: number]; step: [to: Step] }>();

/** The rows as the game wrote them. They depend on the moves alone, so stepping does not rebuild them. */
const rows = computed(() => {
  const parts = props.start.split(" ");
  const blackFirst = parts[1] === "b";
  const first = Number(parts[5] ?? 1);
  const shift = blackFirst ? 1 : 0;
  const out: { number: number; a: string | null; b: string | null; pa: number; pb: number }[] = [];
  const total = props.sans.length + shift;
  for (let i = 0; i < total; i += 2) {
    const a = i - shift;
    const b = i + 1 - shift;
    out.push({
      number: first + i / 2,
      a: props.sans[a] ?? null,
      b: props.sans[b] ?? null,
      pa: a + 1,
      pb: b + 1,
    });
  }
  return out;
});

/** The move that takes the tab stop: the one on the board, or the first before any move is played. */
const tabPly = computed(() => Math.min(Math.max(props.cursor, 1), props.sans.length));

/** What a row needs: its moves, where the board stands against each, and the review's word on each. */
function rowProps(r: (typeof rows.value)[number]) {
  return {
    ...r,
    curA: r.a !== null && r.pa === props.cursor,
    curB: r.b !== null && r.pb === props.cursor,
    futA: r.pa > props.cursor,
    futB: r.pb > props.cursor,
    tabA: r.pa === tabPly.value,
    tabB: r.pb === tabPly.value,
    ma: props.marks?.[r.pa - 1],
    mb: props.marks?.[r.pb - 1],
    ha: props.hints?.[r.pa - 1],
    hb: props.hints?.[r.pb - 1],
  };
}

/**
 * Beside the board the list scrolls inside its pane, so only the rows in view are drawn. Stacked under it the page
 * scrolls and the list simply grows with the game.
 */
const stacked = useMedia("(width < 75rem)");

const scroller = ref<{ $el: HTMLElement; scrollToIndex: (index: number) => void }>();
const plain = ref<HTMLElement>();
const box = () => (scroller.value?.$el as HTMLElement | undefined) ?? plain.value;
/** Scrolls to the row of the move on the board (the top, at the start), for a move far out of the rows drawn. */
function reveal() {
  const index = props.cursor === 0 ? 0 : rows.value.findIndex((r) => r.pa === props.cursor || r.pb === props.cursor);
  if (index >= 0) scroller.value?.scrollToIndex(index);
}

const root = ref<HTMLElement>();

/** A click on a move goes to it; the rows hold no listeners of their own. */
function onClick(e: MouseEvent) {
  const id = (e.target as Element).closest<HTMLElement>("[data-id]")?.dataset["id"];
  if (id !== undefined) emit("go", Number(id));
}

/**
 * The arrow keys, Home and End step the board past the end of a row, as they do from anywhere else; Up and Down go a
 * whole move (two plies) back or forward. Enter and Space press the move in focus.
 */
function onKeys(e: KeyboardEvent) {
  const to = stepOf(e);
  const full =
    !to && !e.altKey && !e.ctrlKey && !e.metaKey && !e.shiftKey && (e.key === "ArrowUp" || e.key === "ArrowDown");
  if (!to && !full) return;
  e.preventDefault();
  e.stopPropagation();
  if (e.repeat) return;
  if (to) emit("step", to);
  else {
    const by = e.key === "ArrowUp" ? -2 : 2;
    emit("go", Math.min(Math.max(props.cursor + by, 0), props.sans.length));
  }
  focusCurrent(() => root.value);
}

followCurrent(box, () => props.cursor, reveal);
</script>

<template>
  <!-- eslint-disable-next-line vuejs-accessibility/no-static-element-interactions -- the moves inside are buttons: the click is delegated, the keys are theirs -->
  <div
    ref="root"
    class="moves"
    role="group"
    aria-label="Moves"
    :aria-busy="loading || undefined"
    @click="onClick"
    @keydown.capture="onKeys"
  >
    <EVirtualList
      v-if="!stacked && rows.length"
      ref="scroller"
      v-slot="{ item }"
      :items="rows"
      :row-height="28"
      measure
      item-key="number"
      class="scroll"
    >
      <MoveRow v-bind="rowProps(item)" />
    </EVirtualList>
    <div v-else-if="rows.length" ref="plain" class="plain-list">
      <MoveRow v-for="r in rows" :key="r.number" v-bind="rowProps(r)" />
    </div>
    <div v-else-if="loading" class="wait" aria-label="Loading moves">
      <span v-for="i in 6" :key="i" class="skel wait-row" />
    </div>
    <UiEmpty v-else dense title="No moves yet." class="none" />
  </div>
</template>

<style scoped>
.moves {
  display: flex;
  flex-direction: column;
  min-height: 0;
  height: 100%;
  overscroll-behavior: contain;
}

.scroll,
.plain-list {
  flex: 1 1 auto;
  min-height: 0;
}

.plain-list {
  overflow-y: auto;
  overscroll-behavior: contain;
}

.scroll {
  height: 100%;
}

.wait {
  display: grid;
  gap: var(--sp-1);
  padding: var(--sp-1) var(--sp-3);
}

.wait-row {
  height: var(--h-move);
}

.none {
  padding: var(--sp-3);
}
</style>
