<script setup lang="ts">
import { computed, onMounted, ref, watch } from "vue";
import { useToast } from "@eos-ui/vue";
import { EButton } from "@eos-ui/vue/components/EButton";
import { calibrationTime, isCalibrated, ladder } from "../sheets/levels";
import { useEngines } from "../stores/engines";
import { useLevels } from "../stores/levels";
import { usePolicy } from "../stores/policy";
import { useWork } from "../stores/work";
import UiBadge from "../ui/UiBadge.vue";
import UiEmpty from "../ui/UiEmpty.vue";
import UiSection from "../ui/UiSection.vue";
import UiSeg from "../ui/UiSeg.vue";
import UiTable from "../ui/UiTable.vue";
import Term from "./Term.vue";

/**
 * Levels: the ladder an engine can play at (each engine on the clock has its own; the analysis engine's first),
 * weakest to strongest, each with the rating the app has found for it by playing (or the one it was designed for,
 * drawn quieter), the games behind it, and You where your rating falls. Calibrate levels queues the games that teach
 * that engine's ladder the most, in the background.
 */
const ladderStore = useLevels();
const engines = useEngines();
const policy = usePolicy();
const work = useWork();
const toast = useToast();

/** The engines with a ladder, installed ones, the analysis engine first. */
const withLevels = computed(() =>
  engines.list
    .filter((e) => e.levels)
    .toSorted((a, b) => Number(b.id === engines.analysisId) - Number(a.id === engines.analysisId)),
);
const picked = ref<string | null>(null);
/** The engine whose ladder is shown: the one picked, else the analysis engine. */
const engineId = computed(() =>
  picked.value && withLevels.value.some((e) => e.id === picked.value) ? picked.value : engines.analysisId,
);
const engineName = computed(() => engines.byId(engineId.value)?.name ?? "The analysis engine");
const engineOptions = computed(() => withLevels.value.map((e) => ({ value: e.id, label: e.name })));

const failed = ref<string | null>(null);
async function read() {
  failed.value = null;
  try {
    await Promise.all([ladderStore.loadLevels(engineId.value), ladderStore.loadPlan(3, engineId.value)]);
  } catch (e) {
    failed.value = e instanceof Error ? e.message : String(e);
  }
}
onMounted(() => {
  if (!engines.loaded) void engines.load().catch(() => {});
  void read();
});
watch(engineId, () => void read());

const overview = computed(() => ladderStore.of(engineId.value));
const levels = computed(() => overview.value?.levels ?? []);
const rungs = computed(() => (overview.value ? ladder(overview.value) : []));
/** One scale for every row: from the lowest end of any interval to the highest, so a wide interval is seen as wide. */
const span = computed(() => {
  const ends = rungs.value.flatMap((r) => [r.low ?? r.elo, r.high ?? r.elo]);
  const low = Math.min(...ends);
  const high = Math.max(...ends);
  return { low, width: Math.max(1, high - low) };
});
const at = (elo: number) => `${Math.min(100, Math.max(0, ((elo - span.value.low) / span.value.width) * 100))}%`;
/** An interval as a bar from its low end to its high end. */
const whisker = (r: { low: number | null; high: number | null }) =>
  r.low == null || r.high == null
    ? undefined
    : { insetInlineStart: at(r.low), inlineSize: `${Math.max(0.5, ((r.high - r.low) / span.value.width) * 100)}%` };
const standingNote = {
  found: undefined,
  provisional: "Provisional: few games so far, or a wide margin. Calibration is still narrowing it.",
  estimate: "Not played yet: the rating it was designed for",
} as const;
const provisional = computed(() => rungs.value.some((r) => r.kind === "level" && r.standing !== "found"));

/** Calibration work in the queue or running: one at a time is enough (they share one lane). */
const calibrating = computed(() => [...work.running, ...work.queued].find((w) => w.kind === "calibrate"));
/** Games calibrate in pairs on two cores each. */
const slots = computed(() => Math.max(1, Math.floor((policy.status?.cores ?? 8) / 4)));
const time = computed(() => calibrationTime(ladderStore.plan, levels.value, slots.value));
const any = computed(() => levels.value.some(isCalibrated));

const columns = [
  { key: "name", label: "Level" },
  { key: "bar", label: "Place on the ladder", width: "40%" },
  { key: "text", label: "Rating", fig: true },
  { key: "games", label: "Games", fig: true, hide: "sm" as const },
];

const busy = ref(false);
async function calibrate() {
  busy.value = true;
  try {
    const w = await ladderStore.calibrate(3, engineId.value);
    toast.show({ title: "Calibration queued", message: w.title, tone: "success" });
  } catch (e) {
    toast.show({
      title: "Calibration was not queued",
      message: e instanceof Error ? e.message : String(e),
      tone: "warning",
    });
  } finally {
    busy.value = false;
  }
}
</script>

<template>
  <UiSection title="Levels">
    <template #aside>
      <EButton
        variant="outline"
        size="sm"
        :loading="busy"
        :disabled="!!calibrating || !ladderStore.plan.length"
        @click="calibrate"
      >
        Calibrate levels
      </EButton>
    </template>
    <template #note>
      {{ engineName }} at a strength you choose, rated by <Term k="calibration" text="calibration" />.
      <template v-if="engines.byId(engineId)?.multipv === false">
        It has no MultiPV, so each level plays its node budget alone.
      </template>
      <template v-if="calibrating">{{ calibrating.title }} is running.</template>
      <template v-else-if="time">{{ time }} in the background.</template>
    </template>
    <UiSeg
      v-if="engineOptions.length > 1"
      class="pick"
      :model-value="engineId"
      label="Engine"
      :options="engineOptions"
      @update:model-value="picked = $event"
    />
    <UiEmpty v-if="failed" tone="error" title="Levels did not load" :body="failed">
      <template #actions><EButton variant="outline" @click="read">Try again</EButton></template>
    </UiEmpty>
    <UiTable
      v-else
      :label="`The levels of ${engineName}`"
      :columns="columns"
      :rows="rungs"
      row-key="id"
      :loading="!overview"
    >
      <template #head-bar><span class="sr-only">Place on the ladder</span></template>
      <template #cell-name="{ row }">
        <span class="name" :class="{ 'is-you': row.kind === 'you' }">{{ row.name }}</span>
        <UiBadge v-if="row.near" tone="info">Near you</UiBadge>
        <UiBadge v-if="row.standing === 'provisional'" :title="standingNote.provisional">Provisional</UiBadge>
        <UiBadge v-else-if="row.standing === 'estimate'" :title="standingNote.estimate">Estimate</UiBadge>
      </template>
      <template #cell-bar="{ row }">
        <span class="track" aria-hidden="true">
          <span
            v-if="whisker(row)"
            class="range"
            :class="{ 'is-you': row.kind === 'you', 'is-loose': row.standing !== 'found' }"
            :style="whisker(row)"
          ></span>
          <span
            class="dot"
            :class="{ 'is-you': row.kind === 'you', 'is-loose': row.standing !== 'found' }"
            :style="{ insetInlineStart: at(row.elo) }"
          ></span>
        </span>
      </template>
      <template #cell-text="{ row }">
        <span :class="{ guess: row.standing !== 'found' }" :title="standingNote[row.standing]">
          {{ row.text }}
        </span>
      </template>
      <template #cell-games="{ row }">{{ row.games || "–" }}</template>
    </UiTable>
    <p v-if="provisional && !failed" class="hint">
      Each bar is the rating's 95 % interval and the dot its best estimate; a hollow dot and a ≈ mean the level is still
      being calibrated, so its rating may move.
    </p>
    <p v-if="overview && !any && !calibrating" class="hint">
      No level has played yet, so these are the ratings each was designed for. Play a few games against a level, or
      calibrate.
    </p>
  </UiSection>
</template>

<style scoped>
.pick {
  margin-block-end: var(--sp-3);
}

.name {
  margin-inline-end: var(--sp-2);
}

.name.is-you {
  font-weight: 600;
}

.track {
  position: relative;
  display: block;
  inline-size: 100%;
  block-size: var(--sp-4);
}

/* The rail the interval and the dot sit on: one hairline through the middle. */
.track::before {
  content: "";
  position: absolute;
  inset: calc(50% - 0.5px) 0 auto;
  block-size: var(--e-stroke-hair);
  background: var(--c-line-strong);
}

/* The 95 % interval: a bar from its low end to its high end, lighter when the rating is still being found. */
.range {
  position: absolute;
  inset-block: calc(50% - var(--sp-1) / 2) auto;
  block-size: var(--sp-1);
  border-radius: var(--r-pill);
  background: var(--c-ink-2);
}

.range.is-loose {
  background: color-mix(in oklab, var(--c-ink-2) 35%, var(--c-sunk));
}

.range.is-you {
  background: var(--c-act);
}

/* The best estimate: filled when found by playing, a ring while provisional or only designed for. */
.dot {
  position: absolute;
  inset-block-start: 50%;
  box-sizing: border-box;
  inline-size: var(--sp-2);
  block-size: var(--sp-2);
  translate: -50% -50%;
  border-radius: var(--r-pill);
  background: var(--c-ink);
}

.dot.is-you {
  background: var(--c-act);
}

.dot.is-loose {
  border: var(--e-stroke-focus) solid var(--c-ink-2);
  background: var(--c-pane);
}

.guess {
  color: var(--c-ink-2);
}

.hint {
  margin: var(--sp-2) 0 0;
  color: var(--c-ink-2);
  font: var(--t-meta);
}
</style>
