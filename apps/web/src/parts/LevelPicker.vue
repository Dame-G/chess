<script setup lang="ts">
import { computed, nextTick, onMounted, useTemplateRef, watch } from "vue";
import type { LevelChoice } from "../sheets/levels";
import UiBadge from "../ui/UiBadge.vue";

/** A row of the picker: a level, or any choice shaped like one, perhaps not to be chosen. */
type Choice = LevelChoice & { disabled?: boolean };

/**
 * Which strength to play against: the levels of the ladder from weakest to strongest (or an engine's ratings), each
 * with its rating. The level nearest your rating is marked, a rating still being calibrated is written with a ≈ and
 * its margin, and one that has not played yet is only the rating it was designed for. A disabled row is shown but
 * skipped. One radio group: the arrow keys move and choose; the list scrolls to the chosen row.
 */
const props = defineProps<{ modelValue: string; choices: Choice[]; label: string }>();
const emit = defineEmits<{ "update:modelValue": [value: string] }>();

const rows = useTemplateRef<HTMLButtonElement[]>("rows");

/** The one row in the tab order: the chosen one, else the first. */
const tabbable = computed(() =>
  Math.max(
    0,
    props.choices.findIndex((c) => c.value === props.modelValue),
  ),
);

/** The next row that can be chosen from `from` in `step`'s direction, or `from` when there is none. */
function nextOpen(from: number, step: 1 | -1): number {
  for (let i = from + step; i >= 0 && i < props.choices.length; i += step) if (!props.choices[i]!.disabled) return i;
  return from;
}

function onKey(e: KeyboardEvent, from: number) {
  let to: number;
  if (e.key === "ArrowDown" || e.key === "ArrowRight") to = nextOpen(from, 1);
  else if (e.key === "ArrowUp" || e.key === "ArrowLeft") to = nextOpen(from, -1);
  else if (e.key === "Home") to = nextOpen(-1, 1);
  else if (e.key === "End") to = nextOpen(props.choices.length, -1);
  else return;
  e.preventDefault();
  const next = props.choices[to];
  if (!next || next.disabled) return;
  emit("update:modelValue", next.value);
  void nextTick(() => rows.value?.[to]?.focus());
}

/** Brings the chosen level into view without moving the page. */
function reveal() {
  void nextTick(() => rows.value?.[tabbable.value]?.scrollIntoView({ block: "nearest" }));
}
onMounted(reveal);
watch(() => [props.modelValue, props.choices.length], reveal);
</script>

<template>
  <div class="picker" role="radiogroup" :aria-label="label">
    <template v-for="(c, i) in choices" :key="c.value">
      <button
        ref="rows"
        type="button"
        class="row"
        :class="{ 'is-on': c.value === modelValue, 'is-loose': c.standing !== 'found' }"
        role="radio"
        :aria-checked="c.value === modelValue"
        :disabled="c.disabled"
        :tabindex="i === tabbable ? 0 : -1"
        @click="emit('update:modelValue', c.value)"
        @keydown="onKey($event, i)"
      >
        <span class="dot" aria-hidden="true" />
        <span class="name">{{ c.name }}</span>
        <UiBadge v-if="c.near" tone="info">About your strength</UiBadge>
        <span
          class="rating num"
          :title="
            c.standing === 'estimate'
              ? 'Not played yet: the rating it was designed for'
              : c.standing === 'provisional'
                ? 'Provisional: calibration is still narrowing it'
                : undefined
          "
          >{{ c.text }}</span
        >
      </button>
    </template>
  </div>
</template>

<style scoped>
.picker {
  display: grid;
  box-sizing: border-box;
  max-block-size: calc(7.5 * var(--h-control));
  overflow-y: auto;
  border: var(--e-stroke-hair) solid var(--c-line-strong);
  border-radius: var(--r-1);
}

.row {
  display: flex;
  align-items: center;
  gap: var(--sp-2);
  box-sizing: border-box;
  min-block-size: var(--h-control);
  padding: 0 var(--sp-3);
  border: 0;
  border-block-end: var(--e-stroke-hair) solid var(--c-line);
  background: transparent;
  color: var(--c-ink);
  font: var(--t-body);
  text-align: start;
  cursor: pointer;
}

.row:last-child {
  border-block-end: 0;
}

.row:disabled {
  color: var(--c-ink-2);
  cursor: not-allowed;
}

.row:hover:not(.is-on, :disabled) {
  background: var(--c-raise);
}

.row:focus-visible {
  outline: var(--focus-ring);
  outline-offset: calc(-1 * var(--e-stroke-focus));
}

.row.is-on {
  background: var(--c-sunk);
  font-weight: 600;
}

/* The radio: a ring, with the action colour inside it for the chosen one. */
.dot {
  flex: none;
  box-sizing: border-box;
  inline-size: var(--sp-3);
  block-size: var(--sp-3);
  border: var(--e-stroke-focus) solid var(--c-line-strong);
  border-radius: var(--r-pill);
}

.is-on .dot {
  border-color: var(--c-act);
  background: radial-gradient(circle, var(--c-act) 45%, transparent 50%);
}

.name {
  min-inline-size: 0;
}

.rating {
  margin-inline-start: auto;
  font: var(--t-fig);
  white-space: nowrap;
}

.is-loose .rating {
  color: var(--c-ink-2);
}
</style>
