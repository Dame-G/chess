<script setup lang="ts">
import { computed } from "vue";
import { EToggletip } from "@eos-ui/vue/components/EToggletip";
import type { Calibration } from "../api/queries";
import { timeControl } from "../format";
import { glossary } from "../glossary";

/**
 * One quiet line on how a clock was played: the written clock is stretched to this machine's speed, as OpenBench
 * does, so results compare with Reckless's own. While a clock test is still measuring, it says so.
 */
const props = defineProps<{
  scaling?: Calibration | null;
  /** Whether it is a clock test (not a node count) that is running now, so the speed is being measured. */
  measuring?: boolean;
}>();

const played = computed(() => (props.scaling ? timeControl(props.scaling.played) : ""));
const k = (n: number) => `${Math.round(n / 1000)}k`;
const help = computed(() => {
  const s = props.scaling;
  if (!s) return "";
  return `${glossary.scaled.text} This machine runs ${k(s.measured_nps)} nodes/s with ${s.copies} games at once. Clocks are stretched to match Reckless's reference of ${k(s.reference_nps)}, so results compare with upstream's (×${s.scale.toFixed(2)}).`;
});
</script>

<template>
  <p v-if="scaling" class="scaled">
    <EToggletip label="Scaled clock" title="Scaled clock" inline>
      <template #trigger>Played as {{ played }} on this machine</template>
      {{ help }}
    </EToggletip>
  </p>
  <p v-else-if="measuring" class="scaled">Measuring this machine's speed…</p>
</template>

<style scoped>
.scaled {
  margin: 0;
  color: var(--e-muted);
  font: var(--e-type-small);
}
</style>
