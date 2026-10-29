<script setup lang="ts">
import { computed } from "vue";
import { formatNumber, NONE } from "../format";
import { GLOSSARY, type TermKey } from "../parts/glossary";
import Term from "../parts/Term.vue";

/**
 * One key figure: its label small and muted above, the figure in mono below, a unit after it and a line under. A
 * missing value is an en dash, never a "0" or a "?". `hint` is the glossary key of a term (the label then explains
 * itself in a toggletip) or, failing that, a plain tooltip. `delta` is a change since before: a signed number takes the
 * outcome colours.
 */
const props = withDefaults(
  defineProps<{
    label: string;
    value?: string | number | null;
    unit?: string;
    delta?: number | string | null;
    tone?: "win" | "loss" | "warn" | "info";
    hint?: string;
    size?: "md" | "lg" | "xl";
    sub?: string;
    /** Decimals for a numeric `value`; none for whole numbers, one otherwise. */
    digits?: number;
  }>(),
  { size: "md" },
);

const term = computed(() => (props.hint && props.hint in GLOSSARY ? (props.hint as TermKey) : null));

const shown = computed(() => {
  const v = props.value;
  if (v == null || v === "") return null;
  return typeof v === "number" ? formatNumber(v, props.digits ?? (Number.isInteger(v) ? 0 : 1)) : v;
});

const deltaText = computed(() => {
  const d = props.delta;
  if (d == null) return null;
  if (typeof d === "string") return d;
  return d === 0 ? "0" : `${d > 0 ? "+" : "−"}${formatNumber(Math.abs(d), Number.isInteger(d) ? 0 : 1)}`;
});
const deltaTone = computed(() =>
  typeof props.delta === "number" && props.delta !== 0 ? (props.delta > 0 ? "win" : "loss") : null,
);
</script>

<template>
  <dl class="ui-stat" :class="`ui-stat--${size}`">
    <dt class="ui-stat-label" :title="term ? undefined : hint">
      <slot name="label">
        <Term v-if="term" :k="term" :text="label" />
        <span v-else :class="{ 'is-hinted': hint }">{{ label }}</span>
      </slot>
    </dt>
    <dd class="ui-stat-value" :class="tone && `ui-stat-value--${tone}`">
      <slot>
        <template v-if="shown != null">{{ shown }}</template>
        <template v-else>
          <span class="ui-stat-none" aria-hidden="true">{{ NONE }}</span>
          <span class="sr-only">no value</span>
        </template>
      </slot>
      <span v-if="unit && shown != null" class="ui-stat-unit">{{ unit }}</span>
      <span v-if="deltaText" class="ui-stat-delta" :class="deltaTone && `ui-stat-delta--${deltaTone}`">{{
        deltaText
      }}</span>
    </dd>
    <dd v-if="sub || $slots['sub']" class="ui-stat-sub">
      <slot name="sub">{{ sub }}</slot>
    </dd>
  </dl>
</template>

<style scoped>
.ui-stat {
  display: grid;
  align-content: start;
  gap: var(--sp-1);
  min-width: 0;
  margin: 0;
}

.ui-stat-label {
  color: var(--c-ink-2);
  font: var(--t-meta);
}

.ui-stat-label .is-hinted {
  text-decoration: underline dotted var(--c-ink-3);
  text-underline-offset: 0.2em;
}

.ui-stat-value {
  display: flex;
  flex-wrap: wrap;
  align-items: baseline;
  gap: 0 var(--sp-1);
  margin: 0;
  color: var(--c-ink);
  font: var(--t-fig-lg);
  font-variant-numeric: tabular-nums;
}

.ui-stat--lg .ui-stat-value {
  font-size: var(--e-text-2xl);
}

.ui-stat--xl .ui-stat-value {
  font: var(--t-fig-xl);
}

.ui-stat-value--win {
  color: var(--c-win);
}

.ui-stat-value--loss {
  color: var(--c-loss);
}

.ui-stat-value--warn {
  color: var(--c-warn);
}

.ui-stat-value--info {
  color: var(--c-info);
}

.ui-stat-none {
  color: var(--c-ink-3);
}

.ui-stat-unit,
.ui-stat-delta {
  color: var(--c-ink-2);
  font: var(--t-meta);
}

.ui-stat-delta {
  font: var(--t-fig);
  font-size: var(--e-text-xs);
}

.ui-stat-delta--win {
  color: var(--c-win);
}

.ui-stat-delta--loss {
  color: var(--c-loss);
}

.ui-stat-sub {
  margin: 0;
  color: var(--c-ink-2);
  font: var(--t-meta);
}
</style>
