<script setup lang="ts">
import { computed, onMounted, ref } from "vue";
import { useToast } from "@eos-ui/vue";
import { EButton } from "@eos-ui/vue/components/EButton";
import { ENotice } from "@eos-ui/vue/components/ENotice";
import { todayText } from "../board/drill";
import { plural } from "../format";
import { useStartDrills } from "../shell/drillFlow";
import { useDrills } from "../stores/drills";
import type { ChartSeries } from "./chart";
import LineChart from "./LineChart.vue";

/**
 * Drills on the You sheet: today's set and a way to start it, how your hit rate moves, and, while there are few, a way
 * to make more from the mistakes in your reviewed games. Local practice; hidden while you play on Lichess.
 */
const drills = useDrills();
const toast = useToast();
const startDrills = useStartDrills();

const failed = ref<string | null>(null);
onMounted(() => drills.load().catch((e: unknown) => (failed.value = e instanceof Error ? e.message : String(e))));

/** Below this many drills the sheet offers to make more. */
const FEW = 8;

const today = computed(() => drills.today);
const count = computed(() => today.value?.drills.length ?? 0);
const few = computed(() => drills.loaded && drills.total < FEW);

const trend = computed(() => drills.trend.filter((p) => p.answers > 0));
const x = computed(() => trend.value.map((p) => Math.round(new Date(`${p.date}T12:00:00Z`).getTime() / 1000)));
const series = computed<ChartSeries[]>(() => [
  { label: "Each day", values: trend.value.map((p) => p.rate * 100), color: "--e-muted", kind: "points" },
  { label: "Last 50 answers", values: trend.value.map((p) => p.rolling * 100), color: "--e-primary", width: 2 },
]);
const hover = ref<number | null>(null);
const readout = computed(() => {
  const p = trend.value[hover.value ?? trend.value.length - 1];
  if (!p) return "";
  return `${p.date} · ${p.passes} of ${plural(p.answers, "answer")} · last 50 answers ${Math.round(p.rolling * 100)}%`;
});

const making = ref(false);
async function make() {
  making.value = true;
  try {
    const n = await drills.make();
    toast.show({
      title: n ? `Made ${plural(n, "drill")}` : "No new drills",
      message: n ? "They come from the mistakes in your reviewed games." : "Review more of your games, then try again.",
      tone: n ? "success" : "info",
    });
  } catch (e) {
    toast.show({
      title: "The drills were not made",
      message: e instanceof Error ? e.message : String(e),
      tone: "warning",
    });
  } finally {
    making.value = false;
  }
}
</script>

<template>
  <section v-if="drills.available" class="sec" aria-labelledby="y-drills">
    <div class="sec-head">
      <h2 id="y-drills" class="part-title">Drills</h2>
      <p class="sec-note">Find the move you missed. They come back as you forget them.</p>
    </div>
    <ENotice v-if="failed" tone="danger" title="Drills did not load">{{ failed }}</ENotice>
    <template v-else-if="drills.loaded">
      <div class="today">
        <p class="count">{{ todayText(today ?? { drills: [], due: 0, new: 0 }) }}</p>
        <div class="sheet-actions">
          <EButton v-if="count" variant="outline" size="sm" @click="startDrills">Start</EButton>
          <EButton v-if="few" variant="outline" size="sm" :loading="making" @click="make">
            Make drills from my mistakes
          </EButton>
        </div>
      </div>
      <p v-if="few && !drills.total" class="sec-note">
        No drills yet. They are made from the mistakes and blunders in games Dameless has reviewed.
      </p>
      <LineChart
        v-if="trend.length > 1"
        label="Your drill hit rate over time"
        :x="x"
        :series="series"
        time
        :height="110"
        :range="[0, 100]"
        :format="(v) => `${Math.round(v)}%`"
        @point="hover = $event"
      >
        <span class="mono readout">{{ readout }}</span>
      </LineChart>
      <p v-else-if="drills.total" class="sec-note">Your hit rate shows here after a few days of drills.</p>
    </template>
  </section>
</template>

<style scoped>
.today {
  display: flex;
  flex-wrap: wrap;
  align-items: center;
  justify-content: space-between;
  gap: var(--e-gap-snug) var(--e-gap-base);
}

.count {
  margin: 0;
  font-weight: 600;
}

.readout {
  font-size: 0.8125rem;
}
</style>
