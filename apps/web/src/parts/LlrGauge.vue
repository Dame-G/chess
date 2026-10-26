<script setup lang="ts">
import { computed } from "vue";
import type { Llr } from "../api/types";
import { signed } from "../format";
import { gaugePosition } from "../sheets/idea";
import Term from "./Term.vue";

/**
 * The evidence in a test, between its two lines. The marker moves right as the change looks better and left as it
 * does not; reaching the right line keeps it, the left line drops it. Green and red are the two outcomes.
 */
const props = defineProps<{ llr: Llr }>();
const at = computed(() => `${gaugePosition(props.llr) * 100}%`);
const zero = computed(() => `${gaugePosition({ ...props.llr, value: 0 }) * 100}%`);
</script>

<template>
  <div class="gauge">
    <div
      class="track"
      role="meter"
      :aria-valuemin="llr.lower"
      :aria-valuemax="llr.upper"
      :aria-valuenow="llr.value"
      :aria-valuetext="`Evidence ${llr.value.toFixed(2)}, between ${llr.lower.toFixed(2)} (dropped) and ${llr.upper.toFixed(2)} (kept)`"
      aria-label="Evidence so far"
    >
      <span class="zone zone-lose"></span>
      <span class="zone zone-win"></span>
      <span class="zero" :style="{ insetInlineStart: zero }"></span>
      <span class="marker" :style="{ insetInlineStart: at }"></span>
    </div>
    <div class="ends">
      <span class="mono">{{ signed(llr.lower) }} · dropped</span>
      <span class="mono now"><Term k="llr" /> {{ signed(llr.value) }}</span>
      <span class="mono">kept · {{ signed(llr.upper) }}</span>
    </div>
  </div>
</template>

<style scoped>
.gauge {
  display: grid;
  gap: 0.25rem;
}

.track {
  position: relative;
  display: flex;
  block-size: 0.75rem;
  border-radius: var(--e-radius-sm);
  background: var(--e-border);
}

.zone {
  flex: 1 1 0;
  opacity: 0.35;
}

.zone-lose {
  background: var(--e-danger);
  border-start-start-radius: var(--e-radius-sm);
  border-end-start-radius: var(--e-radius-sm);
}

.zone-win {
  background: var(--e-success);
  border-start-end-radius: var(--e-radius-sm);
  border-end-end-radius: var(--e-radius-sm);
}

.zero {
  position: absolute;
  inset-block: 0;
  inline-size: 1px;
  background: var(--e-muted);
}

.marker {
  position: absolute;
  inset-block: -0.25rem;
  inline-size: 3px;
  margin-inline-start: -1.5px;
  border-radius: 1px;
  background: var(--e-ink);
  box-shadow: 0 0 0 2px var(--e-surface);
}

.ends {
  display: flex;
  justify-content: space-between;
  gap: var(--e-gap-snug);
  color: var(--e-muted);
  font-size: 0.75rem;
}

.now {
  color: var(--e-ink);
  font-weight: 600;
}
</style>
