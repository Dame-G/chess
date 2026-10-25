<script setup lang="ts">
import { computed } from "vue";
import type { Test } from "../api/queries";
import { bounds, duration, forecast } from "../format";
import Term from "./Term.vue";

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

const sentence = computed(() => {
  const t = props.test;
  if (t.verdict === "accepted") return "Decided: the change passes this stage.";
  if (t.verdict === "rejected") return "Decided: the change fails this stage.";
  if (!t.running) return "Stopped before a decision.";
  const fc = f.value;
  if (!fc || fc.progress < 0.15) return "Too early to tell. The test keeps playing until the evidence is clear.";
  const way = fc.toward === "accept" ? "passing" : "failing";
  const pct = Math.round(fc.progress * 100);
  const eta = fc.minutes !== undefined ? `, about ${duration(fc.minutes)} to go at this pace` : "";
  return `Leaning towards ${way}: ${pct}% of the way to a decision${eta}.`;
});
</script>

<template>
  <div v-if="b" class="gauge">
    <div class="gauge-labels">
      <span>Fails</span>
      <Term of="llr" />
      <span>Passes</span>
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
        :class="at >= zero ? 'gauge-fill--up' : 'gauge-fill--down'"
        :style="{
          insetInlineStart: `${Math.min(at, zero) * 100}%`,
          width: `${Math.abs(at - zero) * 100}%`,
        }"
      ></div>
      <div class="gauge-marker" :style="{ insetInlineStart: `${at * 100}%` }"></div>
    </div>
    <p class="gauge-sentence">{{ sentence }}</p>
  </div>
</template>

<style scoped>
.gauge {
  display: grid;
  gap: var(--e-gap-snug);
}

.gauge-labels {
  display: flex;
  justify-content: space-between;
  align-items: center;
  color: var(--e-muted);
  font: var(--e-type-meta);
  text-transform: uppercase;
}

.gauge-track {
  position: relative;
  height: 0.875rem;
  border-radius: 999px;
  background: linear-gradient(
    90deg,
    color-mix(in oklab, var(--e-danger) 22%, var(--e-surface)),
    var(--e-surface-tint) 50%,
    color-mix(in oklab, var(--e-success) 22%, var(--e-surface))
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
  border-radius: 999px;
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
  top: 50%;
  width: 1.125rem;
  height: 1.125rem;
  border: 3px solid var(--e-surface);
  border-radius: 50%;
  background: var(--e-ink);
  box-shadow: 0 1px 3px rgb(0 0 0 / 30%);
  transform: translate(-50%, -50%);
  transition: inset-inline-start 0.5s ease;
}

.gauge-sentence {
  margin: 0;
  font: var(--e-type-small);
}

@media (prefers-reduced-motion: reduce) {
  .gauge-fill,
  .gauge-marker {
    transition: none;
  }
}
</style>
