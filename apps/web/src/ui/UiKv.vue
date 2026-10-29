<script setup lang="ts">
import { PhCheck, PhCopy } from "@phosphor-icons/vue";
import { inject, onBeforeUnmount, ref, useTemplateRef } from "vue";
import { kvInline } from "./kv";

/**
 * One row of a `UiKeyValue`: a muted label and its value. `mono` is for a path, a commit or a clock; `copy` adds a
 * button that copies the value's text; `tone` colours the value as an outcome.
 */
const props = defineProps<{ label: string; mono?: boolean; copy?: boolean; tone?: "win" | "loss" | "warn" | "info" }>();

const inline = inject(kvInline, false);
const value = useTemplateRef<HTMLElement>("value");
const copied = ref(false);
let timer: ReturnType<typeof setTimeout> | undefined;

async function copyValue() {
  const text = value.value?.textContent?.trim() ?? "";
  try {
    await navigator.clipboard.writeText(text);
  } catch {
    return;
  }
  copied.value = true;
  clearTimeout(timer);
  timer = setTimeout(() => (copied.value = false), 1500);
}
onBeforeUnmount(() => clearTimeout(timer));
</script>

<template>
  <div class="ui-kv" :class="{ 'ui-kv--inline': inline }">
    <dt class="ui-kv-label" :class="{ 'sr-only': inline }">{{ props.label }}</dt>
    <dd class="ui-kv-value">
      <span ref="value" class="ui-kv-text" :class="[mono && 'mono', tone && `ui-kv-text--${tone}`]"><slot /></span>
      <button
        v-if="copy && !inline"
        type="button"
        class="ui-kv-copy"
        :aria-label="copied ? `Copied ${props.label}` : `Copy ${props.label}`"
        @click="copyValue"
      >
        <PhCheck v-if="copied" aria-hidden="true" />
        <PhCopy v-else aria-hidden="true" />
      </button>
    </dd>
  </div>
</template>

<style scoped>
.ui-kv {
  display: grid;
  grid-template-columns: 9rem minmax(0, 1fr);
  align-items: center;
  gap: 0 var(--sp-3);
  min-height: var(--h-row);
  border-block-end: var(--e-stroke-hair) solid var(--c-line);
}

.ui-kv:last-child {
  border-block-end: 0;
}

.ui-kv-label {
  color: var(--c-ink-2);
  font: var(--t-meta);
}

.ui-kv-value {
  display: flex;
  align-items: center;
  gap: var(--sp-2);
  min-width: 0;
  margin: 0;
  font: var(--t-body);
}

.ui-kv-text {
  min-width: 0;
  overflow-wrap: anywhere;
}

.ui-kv-text.mono {
  font: var(--t-fig);
}

.ui-kv-text--win {
  color: var(--c-win);
}

.ui-kv-text--loss {
  color: var(--c-loss);
}

.ui-kv-text--warn {
  color: var(--c-warn);
}

.ui-kv-text--info {
  color: var(--c-info);
}

.ui-kv-copy {
  display: inline-grid;
  flex: none;
  place-items: center;
  width: var(--h-control);
  height: var(--h-control);
  margin-block: calc(-1 * var(--sp-1));
  padding: 0;
  border: 0;
  border-radius: var(--r-1);
  background: transparent;
  color: var(--c-ink-2);
  font-size: var(--i-md);
  cursor: pointer;
}

.ui-kv-copy:hover {
  background: var(--c-raise);
  color: var(--c-ink);
}

.ui-kv-copy:focus-visible {
  outline: var(--focus-ring);
  outline-offset: calc(-1 * var(--e-stroke-focus));
}

/* Inline: values only, in one muted line, a middle dot between them. The label stays for a screen reader. */
.ui-kv--inline {
  display: inline;
  min-height: 0;
  border: 0;
}

.ui-kv--inline .ui-kv-value {
  display: inline;
  color: var(--c-ink-2);
  font: var(--t-meta);
}

.ui-kv--inline + .ui-kv--inline::before {
  content: " · ";
  color: var(--c-ink-3);
  white-space: pre;
}

/* The list is a container: a narrow one stacks the label over the value. */
@container (width < 30rem) {
  .ui-kv:not(.ui-kv--inline) {
    grid-template-columns: minmax(0, 1fr);
    gap: 0;
    padding-block: var(--sp-1);
  }
}
</style>
