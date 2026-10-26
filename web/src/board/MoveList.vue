<script setup lang="ts">
import { computed, ref } from "vue";
import { ESkeleton } from "@eos-ui/vue/components/ESkeleton";
import { followCurrent } from "./follow";
import MoveRow from "./MoveRow.vue";

const props = defineProps<{
  /** The moves in SAN, in order. */
  sans: string[];
  /** How many moves are on the board. */
  cursor: number;
  /** The FEN the line starts from, or `startpos`: it sets the first move number and who moves first. */
  start: string;
  /** A game is on its way: placeholders instead of "no moves". */
  loading?: boolean;
  /** The line is yours to make, on the free board: the empty list says how to start one. */
  free?: boolean;
}>();
const emit = defineEmits<{ go: [ply: number] }>();

/** The rows as the game wrote them. They depend on the moves alone, so stepping does not rebuild them. */
const rows = computed(() => {
  const parts = props.start.split(" ");
  const blackFirst = props.start !== "startpos" && parts[1] === "b";
  const first = props.start === "startpos" ? 1 : Number(parts[5] ?? 1);
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

const list = ref<HTMLElement>();
followCurrent(list, () => props.cursor);
</script>

<template>
  <div ref="list" class="moves" role="list" aria-label="Moves" :aria-busy="loading || undefined">
    <MoveRow
      v-for="r in rows"
      :key="r.number"
      :number="r.number"
      :a="r.a"
      :b="r.b"
      :pa="r.pa"
      :pb="r.pb"
      :cur-a="r.pa === cursor"
      :cur-b="r.pb === cursor"
      :fut-a="r.pa > cursor"
      :fut-b="r.pb > cursor"
      @go="emit('go', $event)"
    />
    <ESkeleton v-if="loading && !sans.length" :lines="8" label="Loading moves" class="none" />
    <p v-else-if="!sans.length" class="none">
      {{ free ? "No moves yet. Move a piece, or paste a game." : "No moves yet." }}
    </p>
  </div>
</template>

<style scoped>
.moves {
  display: grid;
  grid-template-columns: 3rem 1fr 1fr;
  align-content: start;
  height: 100%;
  overflow-y: auto;
  overscroll-behavior: contain;
  font-family: var(--e-font-mono);
}

.none {
  grid-column: 1 / -1;
  margin: 0;
  padding: var(--ws-pad);
  color: var(--e-muted);
  font: var(--e-type-small);
}
</style>
