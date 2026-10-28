<script setup lang="ts">
import { computed, ref } from "vue";
import { useMedia } from "../media";
import { EList } from "@eos-ui/vue/components/EList";
import { ESkeleton } from "@eos-ui/vue/components/ESkeleton";
import { EVirtualList } from "@eos-ui/vue/components/EVirtualList";
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

/** What a row needs: its moves, whether each is still ahead of the board, and the review's word on each. */
function rowProps(r: (typeof rows.value)[number]) {
  return {
    ...r,
    futA: r.pa > props.cursor,
    futB: r.pb > props.cursor,
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
/** Scrolls to the row of the move on the board (the top, at the start), for a move far out of the rows drawn. */
function reveal() {
  const index = props.cursor === 0 ? 0 : rows.value.findIndex((r) => r.pa === props.cursor || r.pb === props.cursor);
  if (index >= 0) scroller.value?.scrollToIndex(index);
}
const root = ref<{ $el: HTMLElement }>();
/** The arrow keys, Home and End step the board past the end of a row, as they do from anywhere else. */
function onKeys(e: KeyboardEvent) {
  const to = stepOf(e);
  if (!to) return;
  e.preventDefault();
  e.stopPropagation();
  if (e.repeat) return;
  emit("step", to);
  focusCurrent(() => root.value?.$el);
}

followCurrent(
  () => scroller.value?.$el as HTMLElement | undefined,
  () => props.cursor,
  reveal,
);
</script>

<template>
  <EList
    ref="root"
    :model-value="cursor"
    label="Moves"
    flush
    class="moves"
    :aria-busy="loading || undefined"
    follow-focus
    @keydown.capture="onKeys"
    @select="emit('go', Number($event))"
  >
    <EVirtualList
      v-if="!stacked && rows.length"
      ref="scroller"
      v-slot="{ item }"
      :items="rows"
      :row-height="32"
      measure
      item-key="number"
      class="scroll"
    >
      <MoveRow v-bind="rowProps(item)" />
    </EVirtualList>
    <template v-else>
      <MoveRow v-for="r in rows" :key="r.number" v-bind="rowProps(r)" />
    </template>
    <ESkeleton v-if="loading && !sans.length" :lines="8" label="Loading moves" class="none" />
    <p v-else-if="!sans.length" class="none">No moves yet.</p>
  </EList>
</template>

<style scoped>
.moves {
  align-content: start;
  height: 100%;
  overscroll-behavior: contain;
}

/* A move is a short word: its cell is the 2rem tall line of a chess client, not the 2.25rem of a form control. Touch
   keeps eos's full target. */
@media (pointer: fine) {
  .moves {
    --e-tap: 2rem;
  }
}

.scroll {
  height: 100%;
}

.none {
  margin: 0;
  padding: var(--ws-pad);
  color: var(--e-muted);
  font: var(--e-type-small);
}
</style>
