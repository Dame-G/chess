<script setup lang="ts">
/** The one calm line where the engine would be while a game is on at Lichess, whose terms forbid analysis. */
import { PhShieldCheck } from "@phosphor-icons/vue";
</script>

<template>
  <p class="note" role="status">
    <PhShieldCheck :size="16" aria-hidden="true" />
    Analysis is off while you’re playing on Lichess
  </p>
</template>

<style scoped>
.note {
  display: flex;
  align-items: center;
  gap: var(--e-gap-snug);
  margin: 0;
  padding: var(--e-gap-base) var(--ws-pad);
  color: var(--e-muted);
  font: var(--e-type-small);
}

.note :deep(svg) {
  flex: none;
}
</style>
