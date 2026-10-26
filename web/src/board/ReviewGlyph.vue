<script setup lang="ts">
import { computed } from "vue";
import "./review.css";
import { markOf, type Classification } from "./review";

/** The mark after a move, in its colour: `??`, `?!`, `!!`. Moves that earn no mark render nothing. */
const props = defineProps<{ of: Classification | null | undefined }>();
const mark = computed(() => (props.of ? markOf(props.of) : undefined));
</script>

<template>
  <span v-if="mark?.glyph" :class="['gl', `rv-${mark.tone}`]" aria-hidden="true">{{ mark.glyph }}</span>
</template>

<style scoped>
.gl {
  margin-inline-start: 0.125rem;
  color: var(--rv);
  font-weight: 800;
}
</style>
