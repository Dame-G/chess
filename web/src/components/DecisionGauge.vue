<script setup lang="ts">
import { computed } from "vue";
import type { Test } from "../api/queries";
import { bounds, duration, forecast, signed } from "../format";
import Term from "./Term.vue";

/** The test's evidence on a track between its two decision lines: fail on the left, pass on the right. */
const props = defineProps<{ test: Test }>();

const b = computed(() => bounds(props.test));
const f = computed(() => (props.test.running ? forecast(props.test) : undefined));

/** Where the evidence sits on the track: 0 at the reject line, 1 at the accept line. */
const at = computed(() => {
  if (!b.value || props.test.llr == null) return 0.5;
  const { reject, accept } = b.value;
  return Math.min(1, Math.max(0, (props.test.llr - reject) / (accept - reject)));
});
const zero = computed(() => (b.value ? -b.value.reject / (b.value.accept - b.value.reject) : 0.5));

/** For screen readers: the page's sentence says the same in words. */
const sentence = computed(() => {
  const t = props.test;
  if (t.verdict === "accepted") return "Decided: passes this stage.";
  if (t.verdict === "rejected") return "Decided: fails this stage.";
  if (!t.running) return "Stopped before a decision.";
  const fc = f.value;
  if (!fc || fc.progress < 0.15) return "Too early to tell.";
  const eta = fc.minutes !== undefined ? `, about ${duration(fc.minutes)} to go` : "";
  return `Leaning towards ${fc.toward === "accept" ? "passing" : "failing"}${eta}.`;
});
const leaning = computed(() => (at.value >= zero.value ? "up" : "down"));
</script>

<template>
  <div v-if="b" class="gauge">
    <div class="gauge-top">
      <span :class="['end', { 'end--on': leaning === 'down' && test.llr != null }]">Fails</span>
      <span class="mid">
        <Term of="llr">Evidence</Term>
        <strong v-if="test.llr != null" :class="['num', 'now', `now--${leaning}`]">{{ signed(test.llr) }}</strong>
      </span>
      <span :class="['end', 'end--right', { 'end--on': leaning === 'up' && test.llr != null }]">Passes</span>
    </div>
    <div
      class="gauge-track"
      role="meter"
      aria-label="Evidence"
      :aria-valuemin="b.reject"
      :aria-valuemax="b.accept"
      :aria-valuenow="test.llr ?? 0"
      :aria-valuetext="sentence"
    >
      <div class="gauge-zero" :style="{ insetInlineStart: `${zero * 100}%` }"></div>
      <div
        class="gauge-fill"
        :class="`gauge-fill--${leaning}`"
        :style="{
          insetInlineStart: `${Math.min(at, zero) * 100}%`,
          width: `${Math.abs(at - zero) * 100}%`,
        }"
      ></div>
      <div class="gauge-marker" :style="{ insetInlineStart: `${at * 100}%` }"></div>
    </div>
    <div class="gauge-scale num" aria-hidden="true">
      <span>{{ signed(b.reject) }}</span>
      <span class="gauge-scale-zero" :style="{ insetInlineStart: `${zero * 100}%` }">0</span>
      <span>{{ signed(b.accept) }}</span>
    </div>
  </div>
</template>

<style scoped>
.gauge {
  display: grid;
  gap: var(--e-gap-snug);
}

.gauge-top {
  display: grid;
  grid-template-columns: 1fr auto 1fr;
  align-items: baseline;
  gap: var(--e-gap-base);
  font: var(--e-type-small);
}

.end {
  color: var(--e-muted);
  font: var(--e-type-meta);
  letter-spacing: 0.04em;
  text-transform: uppercase;
}

.end--right {
  text-align: end;
}

.end--on {
  color: var(--e-ink);
  font-weight: 700;
}

.mid {
  display: inline-flex;
  align-items: baseline;
  gap: var(--e-gap-snug);
}

.now {
  font-size: var(--e-text-md, 1rem);
}

.now--up {
  color: var(--e-success);
}

.now--down {
  color: var(--e-danger);
}

.gauge-track {
  position: relative;
  height: 0.75rem;
  margin-block: var(--e-gap-tight);
  border-radius: var(--e-radius-sm);
  background: linear-gradient(
    90deg,
    color-mix(in oklab, var(--e-danger) 24%, var(--e-surface)),
    var(--e-surface-tint) 50%,
    color-mix(in oklab, var(--e-success) 24%, var(--e-surface))
  );
  box-shadow: inset 0 0 0 1px color-mix(in oklab, var(--e-border) 50%, transparent);
}

.gauge-zero {
  position: absolute;
  top: -0.25rem;
  bottom: -0.25rem;
  width: 1px;
  background: var(--e-border-strong);
}

.gauge-fill {
  position: absolute;
  top: 0.1875rem;
  bottom: 0.1875rem;
  border-radius: 1px;
  transition:
    width 0.5s ease,
    inset-inline-start 0.5s ease;
}

.gauge-fill--up {
  background: var(--e-success);
}

.gauge-fill--down {
  background: var(--e-danger);
}

.gauge-marker {
  position: absolute;
  top: -0.375rem;
  bottom: -0.375rem;
  width: 4px;
  margin-inline-start: -2px;
  border-radius: 2px;
  background: var(--e-ink);
  box-shadow: 0 0 0 2px var(--e-surface);
  transition: inset-inline-start 0.5s ease;
}

.gauge-scale {
  position: relative;
  display: flex;
  justify-content: space-between;
  color: var(--e-muted);
  font-size: var(--e-text-xs, 0.75rem);
}

.gauge-scale-zero {
  position: absolute;
  transform: translateX(-50%);
}

@media (prefers-reduced-motion: reduce) {
  .gauge-fill,
  .gauge-marker {
    transition: none;
  }
}
</style>
