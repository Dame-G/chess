<script setup lang="ts">
import { computed } from "vue";
import { formatResult, NONE, type Result } from "../format";
import type { Side } from "../api/types";

/**
 * A game's result. From your side when `you` is set (win green, loss red, draw muted); a stranger's game,
 * has no side to take and stays ink. Three characters wide and right-aligned, so a column of them lines up.
 * `score` writes `1-0`, `letter` writes W / L / D (and the score where there is no `you`), `dot` writes a coloured dot.
 */
const props = withDefaults(
  defineProps<{
    result: Result | null | undefined;
    you?: Side | null;
    format?: "score" | "letter" | "dot";
  }>(),
  { format: "score" },
);

const view = computed(() => formatResult(props.result, props.you));

const text = computed(() => {
  if (!props.result) return NONE;
  if (props.format === "letter" && view.value.tone) return { win: "W", loss: "L", draw: "D" }[view.value.tone];
  return view.value.text;
});

/** In words, for a screen reader and the tooltip: "You won", "Black won", "Draw", "In progress". */
const words = computed(() => {
  const r = props.result;
  if (!r) return "No result";
  if (r === "*") return "In progress";
  if (r === "1/2-1/2") return "Draw";
  if (props.you) return (r === "1-0") === (props.you === "white") ? "You won" : "You lost";
  return r === "1-0" ? "White won" : "Black won";
});
</script>

<template>
  <span class="ui-result" :class="[view.tone && `ui-result--${view.tone}`, `ui-result--${format}`]" :title="words">
    <span class="ui-result-mark" aria-hidden="true">{{ format === "dot" ? "" : text }}</span>
    <span class="sr-only">{{ words }}</span>
  </span>
</template>

<style scoped>
.ui-result {
  display: inline-block;
  min-width: 3ch;
  color: var(--c-ink);
  font: var(--t-fig);
  text-align: end;
  white-space: nowrap;
}

.ui-result--win {
  color: var(--c-win);
}

.ui-result--loss {
  color: var(--c-loss);
}

.ui-result--draw {
  color: var(--c-draw);
}

.ui-result--dot {
  min-width: 0;
}

.ui-result--dot .ui-result-mark {
  display: inline-block;
  width: var(--dot);
  height: var(--dot);
  border-radius: var(--r-pill);
  background: var(--c-ink-3);
}

.ui-result--dot.ui-result--win .ui-result-mark,
.ui-result--dot.ui-result--loss .ui-result-mark,
.ui-result--dot.ui-result--draw .ui-result-mark {
  background: currentcolor;
}
</style>
