<script setup lang="ts">
import { computed, onMounted, ref, watch } from "vue";
import { RouterLink, useRoute, useRouter } from "vue-router";
import { EButton } from "@eos-ui/vue/components/EButton";
import { ELink } from "@eos-ui/vue/components/ELink";
import { ENotice } from "@eos-ui/vue/components/ENotice";
import { ESkeleton } from "@eos-ui/vue/components/ESkeleton";
import { EStamp } from "@eos-ui/vue/components/EStamp";
import type { ChartSeries } from "../parts/chart";
import BotCard from "../parts/BotCard.vue";
import LineChart from "../parts/LineChart.vue";
import Logbook from "../parts/Logbook.vue";
import NewIdea from "../parts/NewIdea.vue";
import Term from "../parts/Term.vue";
import { signed } from "../format";
import { useDameless } from "../stores/dameless";
import { useIdeas } from "../stores/ideas";
import { useVersions } from "../stores/versions";
import { KIND_WORD, progressWord, STATUS_TONE, STATUS_WORD } from "./idea";
import SheetFrame from "./SheetFrame.vue";

/**
 * Dameless: the champion and its strength line over the versions, the ideas running and queued, the logbook of every
 * idea, New idea (one dialog for every kind), and Out in the world: the bot on Lichess.
 */
const dameless = useDameless();
const ideas = useIdeas();
const versions = useVersions();
const route = useRoute();
const router = useRouter();

const failed = ref<string | null>(null);
onMounted(async () => {
  try {
    await dameless.load();
  } catch (e) {
    failed.value = e instanceof Error ? e.message : String(e);
  }
});

const overview = computed(() => dameless.overview);
const champion = computed(() => overview.value?.champion ?? versions.current);
const strength = computed(() => overview.value?.strength ?? []);
const last = computed(() => strength.value.at(-1));

// The strength line: one step per champion version, with its margin around it.
const x = computed(() => strength.value.map((_, i) => i + 1));
const series = computed<ChartSeries[]>(() => [
  {
    label: "Elo over v1",
    values: strength.value.map((p) => p.elo),
    color: "--e-primary",
    kind: "step",
    width: 2,
    band: { low: strength.value.map((p) => p.low), high: strength.value.map((p) => p.high) },
  },
]);
const hover = ref<number | null>(null);
const readout = computed(() => {
  const p = strength.value[hover.value ?? strength.value.length - 1];
  if (!p) return "";
  return `${p.name}: ${signed(p.elo, 1)} Elo over v1, ±${((p.high - p.low) / 2).toFixed(1)}`;
});

const inProgress = computed(() => [...ideas.running, ...ideas.queued]);
/** "Gain · Short games · 412 pairs so far", leaving out a stage that only repeats the kind (a tune is at "Tune"). */
function note(i: (typeof inProgress.value)[number]): string {
  const stage = progressWord(i);
  const detail = ideas.ideas[i.id]?.trials.find((t) => t.status === "running")?.detail;
  return [KIND_WORD[i.kind], stage === KIND_WORD[i.kind] ? "" : stage, detail ?? ""].filter(Boolean).join(" · ");
}

// New idea: opened from the button, or by `/dameless?new=idea` (the command bar's "new idea").
const creating = ref(route.query["new"] === "idea");
watch(
  () => route.query["new"],
  (v) => {
    if (v === "idea") creating.value = true;
  },
);
watch(creating, (open) => {
  if (!open && route.query["new"] === "idea") void router.replace({ path: "/dameless" });
});
</script>

<template>
  <SheetFrame
    title="Dameless"
    description="The engine: its strength over its versions, the ideas being tried, and its games on Lichess."
  >
    <ENotice v-if="failed" tone="danger" title="Dameless did not load" class="gap">{{ failed }}</ENotice>

    <section class="sec" aria-labelledby="d-strength">
      <div class="sec-head">
        <h2 id="d-strength" class="part-title">Strength</h2>
        <p v-if="champion" class="sec-note">
          <Term k="champion" /> <strong class="mono">{{ champion.name }}</strong>
          <span v-if="last">
            · <span class="mono">{{ signed(last.elo, 1) }}</span> <Term k="elo" /> over v1</span
          >
        </p>
      </div>
      <div v-if="!overview" aria-busy="true"><ESkeleton :lines="3" label="Loading the strength line" /></div>
      <LineChart
        v-else-if="strength.length > 1"
        label="Dameless's strength: cumulative Elo over each champion version"
        :x="x"
        :series="series"
        :height="170"
        :x-format="(v) => (Number.isInteger(v) ? `v${v}` : '')"
        :format="(v) => signed(v, 0)"
        @point="hover = $event"
      >
        <span class="mono">{{ readout }}</span>
      </LineChart>
      <p v-else class="sec-note">The line starts once a second version is promoted.</p>
    </section>

    <section class="sec" aria-labelledby="d-ideas">
      <div class="sec-head">
        <h2 id="d-ideas" class="part-title">Ideas in progress</h2>
        <EButton @click="creating = true">New idea</EButton>
      </div>
      <ul v-if="inProgress.length" class="plain progress">
        <li v-for="i in inProgress" :key="i.id" class="item">
          <EStamp size="sm" :tone="STATUS_TONE[i.status]" :label="STATUS_WORD[i.status]" />
          <div class="item-text">
            <ELink :link-component="RouterLink" :to="`/dameless/ideas/${i.id}`" class="item-title">{{ i.title }}</ELink>
            <span class="sec-note">
              {{ note(i) }}
            </span>
          </div>
        </li>
      </ul>
      <p v-else class="sec-note">Nothing is being tried. Start with a new idea.</p>
    </section>

    <section class="sec" aria-labelledby="d-log">
      <h2 id="d-log" class="part-title">Logbook</h2>
      <Logbook />
    </section>

    <section class="sec">
      <BotCard />
    </section>

    <NewIdea v-model:open="creating" />
  </SheetFrame>
</template>

<style scoped>
.gap {
  margin-block-end: var(--e-gap-base);
}

.progress {
  gap: 0;
}

.item {
  display: grid;
  grid-template-columns: 5.5rem minmax(0, 1fr);
  align-items: center;
  gap: var(--e-gap-snug);
  padding-block: var(--e-gap-snug);
  border-block-end: var(--line);
}

.item-text {
  display: grid;
  min-width: 0;
}

.item-title {
  font-weight: 600;
}
</style>
