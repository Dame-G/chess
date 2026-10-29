<script setup lang="ts">
import { computed, onMounted, ref } from "vue";
import { useToast } from "@eos-ui/vue";
import { EButton } from "@eos-ui/vue/components/EButton";
import { formatDate, formatPercent, plural } from "../format";
import { useStartDrills } from "../shell/drillFlow";
import { useDrills } from "../stores/drills";
import UiEmpty from "../ui/UiEmpty.vue";
import UiSection from "../ui/UiSection.vue";
import UiStat from "../ui/UiStat.vue";
import UiStats from "../ui/UiStats.vue";
import UiToolbar from "../ui/UiToolbar.vue";
import type { ChartSeries } from "./chart";
import ChartKey from "./ChartKey.vue";
import LineChart from "./LineChart.vue";

/**
 * Drills on the You sheet: how many wait today and a way to start them, your hit rate over time, and, while there are
 * few, a way to make more from the mistakes in your reviewed games. Local practice; hidden while you play on Lichess.
 * `review` asks the sheet to show where games are reviewed.
 */
const emit = defineEmits<{ review: [] }>();

const drills = useDrills();
const toast = useToast();
const startDrills = useStartDrills();

const failed = ref<string | null>(null);
const load = () => {
  failed.value = null;
  return drills.load().catch((e: unknown) => (failed.value = e instanceof Error ? e.message : String(e)));
};
onMounted(load);

/** Below this many drills the sheet offers to make more. */
const FEW = 8;

const today = computed(() => drills.today);
const count = computed(() => today.value?.drills.length ?? 0);
const few = computed(() => drills.loaded && drills.total < FEW);

const trend = computed(() => drills.trend.filter((p) => p.answers > 0));
const x = computed(() => trend.value.map((p) => Math.round(new Date(`${p.date}T12:00:00Z`).getTime() / 1000)));
const series = computed<ChartSeries[]>(() => [
  { label: "Each day", values: trend.value.map((p) => p.rate * 100), color: "--c-ink-3", kind: "points" },
  { label: "Last 50 answers", values: trend.value.map((p) => p.rolling * 100), color: "--c-info", width: 2 },
]);
const keys = [
  { label: "Each day", shape: "dot" as const, color: "--c-ink-3" },
  { label: "Last 50 answers", shape: "line" as const, color: "--c-info" },
];
const hover = ref<number | null>(null);
const readout = computed(() => {
  const p = trend.value[hover.value ?? -1];
  if (!p) return "";
  return `${formatDate(`${p.date}T12:00:00Z`)} · ${p.passes} of ${plural(p.answers, "answer")} · last 50 answers ${Math.round(p.rolling * 100)}%`;
});
/** Where the hit rate stands now: the rolling rate at the last day with answers. */
const hitRate = computed(() => {
  const p = trend.value[trend.value.length - 1];
  return p ? formatPercent(p.rolling * 100, 0) : null;
});
const dueText = computed(() => {
  const t = today.value;
  if (!t || !t.drills.length) return "Nothing due";
  return [t.due ? `${t.due} due` : "", t.new ? `${t.new} new` : ""].filter(Boolean).join(", ");
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
  <UiSection v-if="drills.available" id="y-drills" title="Drills">
    <template #note>Find the move you missed. They come back as you forget them.</template>
    <UiEmpty v-if="failed" tone="error" title="Drills did not load" :body="failed">
      <template #actions>
        <EButton variant="outline" @click="load">Try again</EButton>
      </template>
    </UiEmpty>
    <template v-else-if="drills.loaded">
      <UiEmpty v-if="!drills.total" title="No drills yet" body="Review your games, and your mistakes become drills.">
        <template #actions>
          <EButton :loading="making" @click="make">Make drills from my mistakes</EButton>
          <EButton variant="quiet" @click="emit('review')">Review my games</EButton>
        </template>
      </UiEmpty>
      <template v-else>
        <UiStats>
          <UiStat label="Today" :value="count" :sub="dueText" size="lg" />
          <UiStat label="Hit rate" :value="hitRate" hint="Your share of right answers over the last 50" size="lg" />
          <UiStat label="Drills" :value="drills.total" size="lg" />
        </UiStats>
        <UiToolbar>
          <EButton v-if="count" @click="startDrills">Practice {{ count }}</EButton>
          <EButton v-if="few" variant="outline" :loading="making" @click="make">Make more from my mistakes</EButton>
        </UiToolbar>
        <div v-if="trend.length > 1" class="chart">
          <ChartKey :items="keys" :readout="hover == null ? undefined : readout" />
          <LineChart
            label="Your drill hit rate over time"
            :x="x"
            :series="series"
            time
            :height="110"
            :range="[0, 100]"
            :format="(v) => `${Math.round(v)}%`"
            @point="hover = $event"
          />
        </div>
        <UiEmpty v-else dense title="Your hit rate shows here after a few days of drills." />
      </template>
    </template>
    <div v-else class="skel load" aria-hidden="true"></div>
  </UiSection>
</template>

<style scoped>
.chart {
  display: grid;
  gap: var(--sp-1);
  min-width: 0;
}

.load {
  block-size: calc(var(--sp-6) * 3);
}
</style>
