<script setup lang="ts">
import { computed, onMounted, ref } from "vue";
import { useToast } from "@eos-ui/vue";
import { EButton } from "@eos-ui/vue/components/EButton";
import { calibrationTime, isCalibrated, ladder } from "../sheets/levels";
import { useDameless } from "../stores/dameless";
import { usePolicy } from "../stores/policy";
import { useWork } from "../stores/work";
import UiBadge from "../ui/UiBadge.vue";
import UiEmpty from "../ui/UiEmpty.vue";
import UiSection from "../ui/UiSection.vue";
import UiTable from "../ui/UiTable.vue";
import Term from "./Term.vue";

/**
 * Levels: the ladder Dameless can play at, weakest to strongest, each with the rating the app has found for it by
 * playing (or the one it was designed for, drawn quieter), the games behind it, and You where your rating falls.
 * Calibrate levels queues the games that teach the most, in the background.
 */
const dameless = useDameless();
const policy = usePolicy();
const work = useWork();
const toast = useToast();

const failed = ref<string | null>(null);
async function read() {
  failed.value = null;
  try {
    await Promise.all([dameless.loadLevels(), dameless.loadPlan()]);
  } catch (e) {
    failed.value = e instanceof Error ? e.message : String(e);
  }
}
onMounted(read);

const overview = computed(() => dameless.levels);
const levels = computed(() => overview.value?.levels ?? []);
const rungs = computed(() => (overview.value ? ladder(overview.value) : []));
/** The bars span the ladder: the weakest rung keeps a stub, the strongest fills the cell. */
const span = computed(() => {
  const elos = rungs.value.map((r) => r.elo);
  const high = Math.max(...elos);
  const low = Math.min(...elos) - (high - Math.min(...elos)) * 0.12;
  return { low, width: Math.max(1, high - low) };
});
const share = (elo: number) => `${Math.max(2, ((elo - span.value.low) / span.value.width) * 100)}%`;

/** Calibration work in the queue or running: one at a time is enough. */
const calibrating = computed(() => [...work.running, ...work.queued].find((w) => w.kind === "calibrate"));
/** Games calibrate in pairs on two cores each. */
const slots = computed(() => Math.max(1, Math.floor((policy.status?.cores ?? 8) / 4)));
const time = computed(() => calibrationTime(dameless.plan, levels.value, slots.value));
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
    const w = await dameless.calibrate();
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
        :disabled="!!calibrating || !dameless.plan.length"
        @click="calibrate"
      >
        Calibrate levels
      </EButton>
    </template>
    <template #note>
      Dameless at a strength you choose, rated by <Term k="calibration" text="calibration" />.
      <template v-if="calibrating">{{ calibrating.title }} is running.</template>
      <template v-else-if="time">{{ time }} in the background.</template>
    </template>
    <UiEmpty v-if="failed" tone="error" title="Levels did not load" :body="failed">
      <template #actions><EButton variant="outline" @click="read">Try again</EButton></template>
    </UiEmpty>
    <UiTable v-else label="The levels of Dameless" :columns="columns" :rows="rungs" row-key="id" :loading="!overview">
      <template #head-bar><span class="sr-only">Place on the ladder</span></template>
      <template #cell-name="{ row }">
        <span class="name" :class="{ 'is-you': row.kind === 'you' }">{{ row.name }}</span>
        <UiBadge v-if="row.near" tone="info">Near you</UiBadge>
      </template>
      <template #cell-bar="{ row }">
        <span class="track" aria-hidden="true">
          <span
            class="bar"
            :class="{ 'is-you': row.kind === 'you', 'is-guess': !row.found }"
            :style="{ inlineSize: share(row.elo) }"
          ></span>
        </span>
      </template>
      <template #cell-text="{ row }">
        <span
          :class="{ guess: !row.found }"
          :title="row.found ? undefined : 'Not played yet: the rating it was designed for'"
        >
          {{ row.text }}
        </span>
      </template>
    </UiTable>
    <p v-if="overview && !any && !calibrating" class="hint">
      No level has played yet, so these are the ratings each was designed for. Play a few games against a level, or
      calibrate.
    </p>
  </UiSection>
</template>

<style scoped>
.name {
  margin-inline-end: var(--sp-2);
}

.name.is-you {
  font-weight: 600;
}

.track {
  display: block;
  inline-size: 100%;
  block-size: var(--sp-2);
  border-radius: var(--r-pill);
  background: var(--c-sunk);
}

.bar {
  display: block;
  block-size: 100%;
  border-radius: var(--r-pill);
  background: var(--c-ink-2);
}

.bar.is-guess {
  background: color-mix(in oklab, var(--c-ink-2) 40%, var(--c-sunk));
}

.bar.is-you {
  background: var(--c-act);
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
