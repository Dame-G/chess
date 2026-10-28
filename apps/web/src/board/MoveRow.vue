<script setup lang="ts">
import { computed } from "vue";
import { ECellRow, type CellRowCell } from "@eos-ui/vue/components/ECellRow";
import Glyph from "./Glyph.vue";
import type { Classification } from "../api/types";

/**
 * One line of the move list: the number and up to two moves, as an `ECellRow` in the list around it. Its props are
 * plain values, so a step on the board only re-renders the rows whose state changed, not the whole game.
 */
const props = defineProps<{
  number: number;
  /** White's move and Black's, `null` for the gap before Black's first move in a game that starts on Black's turn. */
  a: string | null;
  b: string | null;
  /** The plies of `a` and `b`, as the board counts them: they name the cells in the list. */
  pa: number;
  pb: number;
  /** Whether each move is still ahead of the one on the board. */
  futA: boolean;
  futB: boolean;
  /** What the review judged each move, and the line it has to say about it; nothing without a review. */
  ma?: Classification | null;
  mb?: Classification | null;
  ha?: string;
  hb?: string;
}>();

/** A gap is a disabled cell with a negative value, so it can never be the board's move. */
const cells = computed<CellRowCell[]>(() => {
  const out: CellRowCell[] = [
    props.a ? { value: props.pa, label: props.a } : { value: -props.number, label: "…", disabled: true },
  ];
  if (props.b) out.push({ value: props.pb, label: props.b });
  return out;
});
</script>

<template>
  <ECellRow :label="`${number}.`" :columns="2" :cells="cells">
    <template #cell="{ cell, index }">
      <span class="mv" :class="{ future: index === 0 ? futA : futB }" :title="(index === 0 ? ha : hb) || undefined">
        {{ cell.label }}<Glyph :of="index === 0 ? ma : mb" />
      </span>
    </template>
  </ECellRow>
</template>

<style scoped>
.mv {
  font-family: var(--e-font-mono);
}

.future {
  opacity: 0.72;
}
</style>
