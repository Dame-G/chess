<script setup lang="ts">
import { computed } from "vue";
import "./review.css";
import type { Classification } from "../api/types";
import { markOf } from "./review";

/**
 * The mark after a move, in its colour: `??`, `?!`, `!!`. Moves that earn no mark render nothing. The glyph is the
 * shape that tells the judgements apart (colour alone does not), and a screen reader hears its name. `badge` sets it
 * on a tinted square, as in a chip.
 */
const props = defineProps<{ of: Classification | null | undefined; badge?: boolean }>();
const mark = computed(() => (props.of ? markOf(props.of) : undefined));
</script>

<template>
  <span v-if="mark?.glyph" class="gl" :class="[`rv-${mark.tone}`, { 'gl--badge': badge }]">
    <span aria-hidden="true">{{ mark.glyph }}</span>
    <span class="sr-only"> {{ mark.label }}</span>
  </span>
</template>

<style scoped>
.gl {
  display: inline-block;
  margin-inline-start: var(--sp-1);
  color: var(--rv);
  font: 800 var(--e-text-xs) / 1 var(--e-font-mono);
}

/* A badge is a fixed square, so a row of them lines up whatever the glyph. */
.gl--badge {
  display: inline-grid;
  min-width: 1.25rem;
  height: 1.25rem;
  margin-inline-start: 0;
  padding-inline: var(--sp-1);
  place-items: center;
  border-radius: var(--r-1);
  background: var(--rv-wash);
  box-sizing: border-box;
}
</style>
