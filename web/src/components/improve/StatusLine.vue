<script setup lang="ts">
import type { Status } from "./improve";

/** A status in a word, the figure behind it, both in the colour of the verdict; a live one pulses. */
defineProps<{ status: Status }>();
</script>

<template>
  <span :class="['result', `result--${status.tone}`]">
    <strong class="label">{{ status.label }}</strong>
    <span v-if="status.figure" class="num">{{ status.figure }}</span>
  </span>
</template>

<style scoped>
.result {
  display: flex;
  gap: var(--e-gap-snug);
  align-items: baseline;
  overflow: hidden;
  font-size: var(--e-text-xs, 0.75rem);
  text-overflow: ellipsis;
  white-space: nowrap;
}

.label {
  flex: none;
  font-weight: 600;
}

.result .num {
  overflow: hidden;
  text-overflow: ellipsis;
}

.result--neutral {
  color: var(--e-muted);
}

.result--success {
  color: var(--e-success);
}

.result--danger {
  color: var(--e-danger);
}

.result--info {
  color: var(--e-primary);
}

.result--info::before {
  content: "";
  display: inline-block;
  width: 0.375rem;
  height: 0.375rem;
  align-self: center;
  border-radius: 50%;
  background: currentcolor;
  animation: pulse 1.4s ease-in-out infinite;
}

@keyframes pulse {
  50% {
    opacity: 0.25;
  }
}

@media (prefers-reduced-motion: reduce) {
  .result--info::before {
    animation: none;
  }
}
</style>
