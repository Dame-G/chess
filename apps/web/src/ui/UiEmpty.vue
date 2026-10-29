<script setup lang="ts">
import { PhWarningCircle } from "@phosphor-icons/vue";
import type { Component } from "vue";

/**
 * Nothing here yet, or something failed: one sentence that teaches the next action and at most one button. Centred in
 * its container, an icon above. `dense` is one muted line inside a section (the title, then the body, then the
 * actions inline). `tone="error"` has a red icon and no red panel.
 */
withDefaults(
  defineProps<{
    icon?: Component;
    title: string;
    body?: string;
    tone?: "neutral" | "error";
    dense?: boolean;
  }>(),
  { tone: "neutral" },
);
</script>

<template>
  <p
    v-if="dense"
    class="ui-empty-dense"
    :class="{ 'is-error': tone === 'error' }"
    :role="tone === 'error' ? 'alert' : undefined"
  >
    {{ title }}
    <template v-if="body">{{ " " }}{{ body }}</template>
    <template v-if="$slots['actions']">{{ " " }}</template>
    <span v-if="$slots['actions']" class="ui-empty-inline"><slot name="actions" /></span>
  </p>
  <div v-else class="ui-empty" :role="tone === 'error' ? 'alert' : undefined">
    <component
      :is="icon ?? (tone === 'error' ? PhWarningCircle : undefined)"
      class="ui-empty-icon"
      :class="{ 'is-error': tone === 'error' }"
      aria-hidden="true"
    />
    <h3 class="ui-empty-title">{{ title }}</h3>
    <p v-if="body" class="ui-empty-body">{{ body }}</p>
    <div v-if="$slots['actions']" class="ui-empty-actions"><slot name="actions" /></div>
  </div>
</template>

<style scoped>
.ui-empty {
  display: grid;
  align-content: center;
  justify-items: center;
  gap: var(--sp-2);
  padding: var(--sp-6) var(--sp-4);
  text-align: center;
}

.ui-empty-icon {
  color: var(--c-ink-3);
  font-size: var(--i-xl);
}

.ui-empty-icon.is-error {
  color: var(--c-loss);
}

.ui-empty-title {
  margin: 0;
  font: var(--t-head);
}

.ui-empty-body {
  max-width: 36ch;
  margin: 0;
  color: var(--c-ink-2);
  font: var(--t-body);
}

.ui-empty-actions {
  display: flex;
  flex-wrap: wrap;
  justify-content: center;
  gap: var(--sp-2);
  margin-block-start: var(--sp-2);
}

.ui-empty-dense {
  margin: 0;
  color: var(--c-ink-2);
  font: var(--t-body);
}

.ui-empty-dense.is-error {
  color: var(--c-loss);
}

.ui-empty-inline {
  display: inline-flex;
  flex-wrap: wrap;
  gap: var(--sp-2);
  margin-inline-start: var(--sp-1);
  vertical-align: middle;
}
</style>
