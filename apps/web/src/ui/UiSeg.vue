<script setup lang="ts" generic="T extends string | number">
import { computed, nextTick, useTemplateRef, type Component } from "vue";

export interface SegOption<V> {
  value: V;
  label: string;
  icon?: Component;
  /** A figure after the label, muted and mono: `Stockfish 12`. */
  count?: number | string;
  disabled?: boolean;
}

/**
 * The one segmented control: a quiet track, the chosen option on the pane with an underline in the action colour. One
 * choice by default (`v-model`, a radio group: arrow keys move and choose); `multiple` lets several be on
 * (`v-model:values`, a group of toggle buttons). Any number of options; a long row scrolls inside the track.
 */
const props = withDefaults(
  defineProps<{
    modelValue?: T;
    values?: T[];
    options: SegOption<T>[];
    /** The group's name for a screen reader; nothing shows it. */
    label: string;
    size?: "sm" | "md";
    /** Fill the width, the options sharing it. */
    full?: boolean;
    multiple?: boolean;
    /** Tighter options a step smaller, sharing the width of a narrow place (five in the games rail). */
    dense?: boolean;
  }>(),
  { size: "md" },
);

const emit = defineEmits<{ "update:modelValue": [value: T]; "update:values": [values: T[]] }>();

const buttons = useTemplateRef<HTMLButtonElement[]>("buttons");

const isOn = (o: SegOption<T>) => (props.multiple ? !!props.values?.includes(o.value) : o.value === props.modelValue);

/** The one option that is in the tab order of a radio group: the chosen one, else the first that can be chosen. */
const tabbable = computed(() => {
  const chosen = props.options.findIndex((o) => !o.disabled && o.value === props.modelValue);
  return chosen >= 0 ? chosen : props.options.findIndex((o) => !o.disabled);
});

function choose(o: SegOption<T>) {
  if (o.disabled) return;
  if (props.multiple) {
    const on = props.values ?? [];
    emit("update:values", on.includes(o.value) ? on.filter((v) => v !== o.value) : [...on, o.value]);
  } else if (o.value !== props.modelValue) {
    emit("update:modelValue", o.value);
  }
}

/** Arrow keys, Home and End move through the radios and choose as they go, as a native radio group does. */
function onKey(e: KeyboardEvent, from: number) {
  if (props.multiple) return;
  const count = props.options.length;
  let step: number;
  let to: number;
  if (e.key === "ArrowRight" || e.key === "ArrowDown") [step, to] = [1, from];
  else if (e.key === "ArrowLeft" || e.key === "ArrowUp") [step, to] = [-1, from];
  else if (e.key === "Home") [step, to] = [1, -1];
  else if (e.key === "End") [step, to] = [-1, count];
  else return;
  e.preventDefault();
  for (let i = 0; i < count; i++) {
    to = (to + step + count) % count;
    const o = props.options[to];
    if (o && !o.disabled) {
      choose(o);
      void nextTick(() => buttons.value?.[to]?.focus());
      return;
    }
  }
}
</script>

<template>
  <div
    class="ui-seg"
    :class="[`ui-seg--${size}`, { 'ui-seg--full': full, 'ui-seg--dense': dense }]"
    :role="multiple ? 'group' : 'radiogroup'"
    :aria-label="label"
  >
    <button
      v-for="(o, i) in options"
      :key="String(o.value)"
      ref="buttons"
      type="button"
      class="ui-seg-option"
      :class="{ 'is-on': isOn(o) }"
      :role="multiple ? undefined : 'radio'"
      :aria-checked="multiple ? undefined : isOn(o)"
      :aria-pressed="multiple ? isOn(o) : undefined"
      :tabindex="multiple || i === tabbable ? 0 : -1"
      :disabled="o.disabled"
      @click="choose(o)"
      @keydown="onKey($event, i)"
    >
      <component :is="o.icon" v-if="o.icon" class="ui-seg-icon" aria-hidden="true" />
      <span class="ui-seg-label" :data-label="o.label">{{ o.label }}</span>
      <span v-if="o.count != null" class="ui-seg-count num">{{ o.count }}</span>
    </button>
  </div>
</template>

<style scoped>
.ui-seg {
  --_h: var(--h-control);

  display: inline-flex;
  box-sizing: border-box;
  max-width: 100%;
  padding: var(--e-stroke-focus);
  overflow-x: auto;
  border-radius: var(--r-1);
  background: var(--c-sunk);
  scrollbar-width: none;
}

.ui-seg--sm {
  --_h: calc(var(--h-control) - var(--sp-1));
}

/* With a finger the track is the target: no padding, the options as tall as any control. */
@media (pointer: coarse) {
  .ui-seg,
  .ui-seg--sm {
    --_h: var(--h-control);

    padding: 0;
  }
}

.ui-seg--full {
  display: flex;
  width: 100%;
}

.ui-seg-option {
  position: relative;
  display: inline-flex;
  flex: none;
  align-items: center;
  justify-content: center;
  gap: var(--sp-1);
  box-sizing: border-box;
  min-height: var(--_h);
  padding: 0 var(--sp-3);
  border: 0;
  border-radius: calc(var(--r-1) - 1px);
  background: transparent;
  color: var(--c-ink-2);
  font: 500 var(--e-text-md) / 1 var(--e-font-sans);
  white-space: nowrap;
  cursor: pointer;
  transition:
    background-color var(--m-fast) var(--m-ease),
    color var(--m-fast) var(--m-ease);
}

.ui-seg--sm .ui-seg-option {
  padding-inline: var(--sp-2);
}

.ui-seg--full .ui-seg-option {
  flex: 1 1 0;
  min-width: 0;
}

.ui-seg--dense {
  flex: 1 1 auto;
}

.ui-seg--dense .ui-seg-option {
  flex: 1 1 auto;
  padding-inline: var(--sp-1);
  font-size: var(--e-text-xs);
}

.ui-seg-option:hover:not(:disabled, .is-on) {
  color: var(--c-ink);
}

.ui-seg-option:focus-visible {
  z-index: var(--e-z-raised);
  outline: var(--focus-ring);
  outline-offset: calc(-1 * var(--e-stroke-focus));
}

.ui-seg-option:disabled {
  color: var(--c-ink-3);
  cursor: not-allowed;
}

.ui-seg-option.is-on {
  background: var(--c-pane);
  color: var(--c-ink);
  font-weight: 600;
}

/* The chosen option's marker: the action colour, two pixels under the label. */
.ui-seg-option.is-on::after {
  content: "";
  position: absolute;
  inset: auto var(--sp-2) var(--sp-1);
  height: var(--e-stroke-focus);
  background: var(--c-act);
}

.ui-seg--sm .ui-seg-option.is-on::after {
  inset-inline: var(--sp-1);
}

/* The label reserves its bold width, so choosing an option never moves its neighbours. */
.ui-seg-label::after {
  content: attr(data-label);
  display: block;
  height: 0;
  overflow: hidden;
  font-weight: 600;
  visibility: hidden;
}

.ui-seg-icon {
  flex: none;
  font-size: var(--i-md);
}

.ui-seg-count {
  color: var(--c-ink-2);
  font: var(--t-fig);
  font-size: var(--e-text-xs);
  font-weight: 500;
}

@media (prefers-reduced-motion: reduce) {
  .ui-seg-option {
    transition: none;
  }
}
</style>
