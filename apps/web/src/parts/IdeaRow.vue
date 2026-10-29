<script setup lang="ts">
import { computed } from "vue";
import { RouterLink } from "vue-router";
import type { Idea } from "../api/types";
import { evidenceLine, gaugeTrial, KIND_WORD, progressWord, STATUS_WORD } from "../sheets/idea";
import { ideaTone } from "../ui/badge";
import UiBadge from "../ui/UiBadge.vue";
import LlrGauge from "./LlrGauge.vue";

/**
 * An idea that is still going, as one row: its status, its title, what kind it is and where it is, one line of
 * evidence and the thin gauge while a trial has games. The whole row opens the idea. Ended ideas are the logbook's rows,
 * which carry the same badge and the same words.
 */
const props = defineProps<{ idea: Idea }>();

const stage = computed(() => {
  const word = progressWord(props.idea);
  return word === KIND_WORD[props.idea.kind] ? "" : word;
});
const meta = computed(() => [KIND_WORD[props.idea.kind], stage.value].filter(Boolean).join(" · "));
const evidence = computed(() => evidenceLine(props.idea));
const trial = computed(() => gaugeTrial(props.idea));
</script>

<template>
  <RouterLink :to="`/dameless/ideas/${idea.id}`" class="row">
    <div class="top">
      <UiBadge :tone="ideaTone[idea.status]">{{ STATUS_WORD[idea.status] }}</UiBadge>
      <span class="title truncate">{{ idea.title }}</span>
    </div>
    <p class="meta">{{ meta }}</p>
    <p v-if="evidence" class="evidence">{{ evidence }}</p>
    <LlrGauge v-if="trial?.llr" :llr="trial.llr" compact class="gauge" />
  </RouterLink>
</template>

<style scoped>
/* A row is a little wider than its text, so its hover wash has room and the text still lines up with the title above. */
.row {
  display: grid;
  gap: var(--sp-1);
  min-width: 0;
  margin-inline: calc(-1 * var(--sp-2));
  padding: var(--sp-2);
  border-radius: var(--r-1);
  color: inherit;
  text-decoration: none;
}

.row:hover {
  background: var(--c-raise);
}

.row:focus-visible {
  outline: var(--focus-ring);
  outline-offset: calc(-1 * var(--e-stroke-focus));
}

.top {
  display: flex;
  align-items: center;
  gap: var(--sp-2);
  min-width: 0;
}

.title {
  min-width: 0;
  font: var(--t-body-strong);
}

.meta {
  margin: 0;
  color: var(--c-ink-2);
  font: var(--t-meta);
}

.evidence {
  margin: 0;
  font: var(--t-body);
  font-variant-numeric: tabular-nums;
}

.gauge {
  margin-block-start: var(--sp-1);
}
</style>
