<script setup lang="ts">
/**
 * A banner row inside a profile: what to do next in a sentence or two on the left, its buttons on the right. `lead`
 * is for the one thing that matters most on the screen (nothing reviewed yet): bigger, on the action's wash. `bare`
 * drops the frame and keeps only the buttons.
 */
defineProps<{ lead?: boolean; bare?: boolean }>();
defineSlots<{ default?(): unknown; actions?(): unknown }>();
</script>

<template>
  <div class="notice" :class="{ 'notice--lead': lead, 'notice--bare': bare }">
    <div v-if="$slots['default'] && !bare" class="text"><slot /></div>
    <div v-if="$slots['actions']" class="actions"><slot name="actions" /></div>
  </div>
</template>

<style scoped>
.notice {
  display: flex;
  flex-wrap: wrap;
  align-items: center;
  gap: var(--sp-2) var(--sp-4);
  padding: var(--sp-3);
  border-radius: var(--r-1);
  background: var(--c-raise);
}

.notice--lead {
  padding: var(--sp-4);
  background: var(--c-act-wash);
}

.notice--bare {
  justify-content: center;
  padding: 0;
  background: transparent;
}

.text {
  display: grid;
  flex: 1 1 var(--w-basis);
  gap: var(--sp-1);
  min-width: 0;
}

.actions {
  display: flex;
  flex-wrap: wrap;
  align-items: center;
  gap: var(--sp-2);
}

.text :deep(.title) {
  margin: 0;
  font: var(--t-body-strong);
}

.notice--lead .text :deep(.title) {
  font: var(--t-head);
}

.text :deep(.cost) {
  margin: 0;
  color: var(--c-ink-2);
  font: var(--t-meta);
}

.notice--lead .text :deep(.cost) {
  max-width: var(--w-measure);
  font: var(--t-body);
}
</style>
