<script setup lang="ts">
import { computed, onMounted, ref } from "vue";
import { EProgress } from "@eos-ui/vue/components/EProgress";
import { ESkeleton } from "@eos-ui/vue/components/ESkeleton";
import type { Idea, TrialStatus } from "../api/types";
import { bytes, duration } from "../format";
import {
  checkpointText,
  lossCurve,
  positionsText,
  rateText,
  SCHEDULES,
  stepPosition,
  trainCost,
  trainCostText,
  presetOf,
} from "../sheets/training";
import { useDameless } from "../stores/dameless";
import { useTraining } from "../stores/training";
import type { ChartSeries } from "./chart";
import LineChart from "./LineChart.vue";
import Term from "./Term.vue";

/**
 * The training stage of a network idea: where the run is, its loss as it falls (from the stored steps, extended live
 * by the run's progress events), how fast the GPU is going, and the checkpoints it keeps. When it has finished, the
 * network it made.
 */
const props = defineProps<{ idea: Idea; status: TrialStatus }>();

const training = useTraining();
const dameless = useDameless();

const failed = ref(false);
onMounted(() => {
  if (!training.loaded) void training.load().catch(() => {});
  void training.loadRun(props.idea.id).catch(() => (failed.value = true));
  if (!dameless.health) void dameless.loadHealth().catch(() => {});
});

const run = computed(() => training.runs[props.idea.id]);
const schedule = computed(() => run.value?.spec.schedule ?? SCHEDULES.quick.schedule);
const steps = computed(() => run.value?.steps ?? []);
const last = computed(() => steps.value.at(-1));
const curve = computed(() => lossCurve(steps.value, schedule.value.batches_per_superbatch));
const series = computed<ChartSeries[]>(() => [
  { label: "Loss", values: curve.value.loss, color: "--e-primary", width: 2 },
]);

/** How far along: superbatches done, as a share of the schedule. */
const fraction = computed(() => {
  const l = last.value;
  if (!l) return 0;
  return Math.min(100, (stepPosition(l, schedule.value.batches_per_superbatch) / schedule.value.superbatches) * 100);
});
const where = computed(() => {
  const l = last.value;
  if (!l) return "";
  return `Superbatch ${l.superbatch} of ${schedule.value.superbatches}`;
});
/** What to say before the first training step: the run is starting, waiting, or ended without one. */
const noSteps = computed(() => {
  if (props.status === "running") return "Starting up: the run is loading its data.";
  if (props.status === "waiting") return "Waiting for the GPU.";
  return "It ended before any training step.";
});
const finished = computed(() => props.status === "passed" || !!run.value?.net);
const running = computed(() => props.status === "running");

const gpu = computed(() => dameless.health?.items.find((i) => i.key === "gpu")?.detail ?? "");
const datasetNames = computed(() => {
  const ids = run.value?.spec.datasets ?? [];
  if (!ids.length) return "All the data in the engine's units";
  return ids.map((id) => training.datasets.find((d) => d.id === id)?.name ?? `dataset ${id}`).join(", ");
});
const parent = computed(() => {
  const id = run.value?.spec.parent;
  return id == null ? "the champion's network" : (training.netName(id) ?? `network ${id}`);
});
const schedulePreset = computed(() => {
  const p = presetOf(schedule.value);
  return p ? SCHEDULES[p].label : "Custom schedule";
});
const remaining = computed(() => {
  const l = last.value;
  if (!l || !running.value || fraction.value <= 0) return "";
  const total = (l.elapsed_seconds / fraction.value) * 100;
  return duration(Math.max(0, total - l.elapsed_seconds));
});
const madeText = computed(() => {
  const net = run.value?.net;
  if (!net) return "";
  const loss = net.run?.final_loss;
  return [
    bytes(net.size_bytes),
    net.run ? `${positionsText(net.run.positions)} positions` : "",
    loss != null ? `ending at a loss of ${loss.toFixed(4)}` : "",
  ]
    .filter(Boolean)
    .join(", ");
});
const hover = ref<number | null>(null);
const readout = computed(() => {
  const i = hover.value;
  if (i == null) return "";
  const x = curve.value.x[i];
  const l = curve.value.loss[i];
  return x == null || l == null ? "" : `superbatch ${x.toFixed(1)} · loss ${l.toFixed(4)}`;
});
</script>

<template>
  <div class="train">
    <p class="detail">
      {{ schedulePreset }}: {{ datasetNames }}, from {{ parent }}.
      <span class="sec-note">{{ trainCostText(trainCost(schedule, run?.spec.keep_checkpoints ?? 2)) }}</span>
    </p>

    <div v-if="!run && !failed" aria-busy="true"><ESkeleton :lines="3" label="Loading the training" /></div>
    <p v-else-if="!steps.length" class="sec-note">{{ noSteps }}</p>
    <template v-else>
      <EProgress :value="fraction" :label="where" />
      <LineChart
        label="The training loss as the run goes on"
        :x="curve.x"
        :series="series"
        :height="150"
        :x-format="(v) => (Number.isInteger(v) ? String(v) : '')"
        :format="(v) => v.toFixed(3)"
        @point="hover = $event"
      >
        <span class="mono readout">{{ readout }}</span>
      </LineChart>
      <dl class="figs figs--small">
        <div class="fig">
          <dt><Term k="loss" /></dt>
          <dd>{{ last!.loss.toFixed(4) }}</dd>
        </div>
        <div class="fig">
          <dt>Speed</dt>
          <dd>
            <small>{{ rateText(last!.positions_per_sec) }}</small>
          </dd>
        </div>
        <div class="fig">
          <dt>{{ finished ? "Took" : "Running for" }}</dt>
          <dd>
            <small
              >{{ duration(last!.elapsed_seconds)
              }}<template v-if="remaining && remaining !== duration(last!.elapsed_seconds)">
                · {{ remaining }} left</template
              ></small
            >
          </dd>
        </div>
        <div v-if="gpu" class="fig">
          <dt>GPU</dt>
          <dd>
            <small>{{ gpu }}</small>
          </dd>
        </div>
      </dl>
    </template>
    <p class="sec-note">
      Checkpoints are saved {{ checkpointText(run?.spec ?? {}) }}. A stopped run picks up from the last one.
    </p>
    <p v-if="run?.net" class="made">
      Made <strong class="mono">{{ run.net.name }}</strong
      >: {{ madeText }}.
    </p>
  </div>
</template>

<style scoped>
.train {
  display: grid;
  gap: var(--e-gap-snug);
  min-width: 0;
}

.detail {
  margin: 0;
  font-weight: 600;
}

.detail .sec-note {
  display: block;
  font-weight: 400;
}

.train .fig {
  align-content: start;
}

.made {
  margin: 0;
}

.readout {
  font-size: 0.8125rem;
}
</style>
