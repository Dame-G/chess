<script setup lang="ts">
/** The search and filters above a library's list. They scroll away with it, so the head above stays one bar tall. */
</script>

<template>
  <div class="tools"><slot /></div>
</template>

<style scoped>
.tools {
  display: grid;
  grid-template-columns: minmax(0, 1fr);
  gap: var(--e-gap-base);
  padding: var(--e-gap-snug) var(--ws-pad, var(--e-pad-control-sm));
  border-block-end: var(--e-stroke-hair) solid color-mix(in oklab, var(--e-border) 35%, transparent);
}
</style>
