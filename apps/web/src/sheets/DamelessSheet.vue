<script setup lang="ts">
import { computed, onMounted, ref, watch } from "vue";
import { RouterLink, useRoute, useRouter } from "vue-router";
import { ESheet } from "@eos-ui/vue/components/ESheet";
import { EButton } from "@eos-ui/vue/components/EButton";
import { ELink } from "@eos-ui/vue/components/ELink";
import { ENotice } from "@eos-ui/vue/components/ENotice";
import { ESkeleton } from "@eos-ui/vue/components/ESkeleton";
import { EStamp } from "@eos-ui/vue/components/EStamp";
import type { ChartSeries } from "../parts/chart";
import BotCard from "../parts/BotCard.vue";
import BrainCard from "../parts/BrainCard.vue";
import LevelsCard from "../parts/LevelsCard.vue";
import ProgressChart from "../parts/ProgressChart.vue";
import LineChart from "../parts/LineChart.vue";
import Logbook from "../parts/Logbook.vue";
import NewIdea from "../parts/NewIdea.vue";
import Term from "../parts/Term.vue";
import { signed } from "../format";
import { useDameless } from "../stores/dameless";
import { useIdeas } from "../stores/ideas";
import { useProgress } from "../stores/progress";
import { useVersions } from "../stores/versions";
import { KIND_WORD, progressWord, STATUS_TONE, STATUS_WORD } from "./idea";
import SheetBack from "./SheetBack.vue";
import { useSheet } from "./useSheet";

/**
 * Dameless: the champion and its strength line over the versions, the ideas running and queued, the logbook of every
 * idea, New idea (one dialog for every kind), and Out in the world: the bot on Lichess.
 */
const sheet = useSheet();
const dameless = useDameless();
const ideas = useIdeas();
const progress = useProgress();
const versions = useVersions();
const route = useRoute();
const router = useRouter();

const failed = ref<string | null>(null);
async function read() {
  failed.value = null;
  try {
    await dameless.load();
  } catch (e) {
    failed.value = e instanceof Error ? e.message : String(e);
  }
}
onMounted(() => {
  void progress.load().catch(() => {});
  void read();
});

const overview = computed(() => dameless.overview);
const champion = computed(() => overview.value?.champion ?? versions.current);
const strength = computed(() => overview.value?.strength ?? []);

// The strength line: one step per champion version, with its margin around it.
const x = computed(() => strength.value.map((_, i) => i + 1));
const series = computed<ChartSeries[]>(() => [
  {
    label: "Elo over v1",
    values: strength.value.map((p) => p.value),
    color: "--e-primary",
    kind: "step",
    width: 2,
    band: { low: strength.value.map((p) => p.low ?? p.value), high: strength.value.map((p) => p.high ?? p.value) },
  },
]);
const hover = ref<number | null>(null);
const readout = computed(() => {
  const p = strength.value[hover.value ?? strength.value.length - 1];
  if (!p) return "";
  const margin = p.high != null && p.low != null ? `, ±${((p.high - p.low) / 2).toFixed(1)}` : "";
  return `${p.label ?? ""}: ${signed(p.value, 1)} Elo over v1${margin}`;
});

/** Both players have a line to draw: yours (rating) and Dameless's (Elo). */
const both = computed(() => {
  const s = progress.chart?.series ?? [];
  return (
    s.some((x) => x.metric === "rating" && x.points.length) && s.some((x) => x.metric === "elo" && x.points.length)
  );
});

/** Champions taken from upstream Reckless, newest first, and the last sync (shown when it failed). */
const fromUpstream = computed(() => (overview.value?.line ?? []).filter((v) => v.upstream));
const lastSync = computed(() => overview.value?.upstream ?? null);
function upstreamLabel(v: (typeof fromUpstream.value)[number]): string {
  const n = v.upstream?.commits.length ?? 0;
  return `${v.name} · from upstream · ${n} commit${n === 1 ? "" : "s"}`;
}

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
  <ESheet
    v-bind="sheet.bind"
    title="Dameless"
    description="The engine: its strength over its versions, the ideas being tried, and its games on Lichess."
  >
    <template #leading><SheetBack :show="sheet.canGoBack" @back="sheet.back" /></template>
    <ENotice v-if="failed" tone="danger" title="Dameless did not load" class="gap">
      {{ failed }}
      <template #actions><EButton variant="outline" @click="read">Try again</EButton></template>
    </ENotice>

    <section class="sec" aria-labelledby="d-strength">
      <div class="sec-head">
        <h2 id="d-strength" class="part-title">Strength</h2>
        <p v-if="champion" class="sec-note">
          <Term k="champion" /> <strong class="mono">{{ champion.name }}</strong>
        </p>
      </div>
      <div v-if="!overview && !failed" aria-busy="true">
        <ESkeleton :lines="3" label="Loading the strength line" />
      </div>
      <p v-else-if="!overview" class="sec-note">The strength line is not available.</p>
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

    <section v-if="progress.chart && both" class="sec" aria-labelledby="d-progress">
      <div class="sec-head">
        <h2 id="d-progress" class="part-title">You and Dameless</h2>
        <ELink :link-component="RouterLink" to="/you">Your progress</ELink>
      </div>
      <ProgressChart :chart="progress.chart" compact />
    </section>

    <LevelsCard />

    <section class="sec" aria-labelledby="d-ideas">
      <div class="sec-head">
        <h2 id="d-ideas" class="part-title">Ideas in progress</h2>
        <EButton @click="creating = true">New idea</EButton>
      </div>
      <div v-if="!ideas.loaded && !failed" aria-busy="true"><ESkeleton :lines="2" label="Loading the ideas" /></div>
      <p v-else-if="!ideas.loaded" class="sec-note">The ideas are not available.</p>
      <ul v-else-if="inProgress.length" class="plain progress">
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
      <ul v-if="fromUpstream.length" class="plain upstream">
        <li v-for="v in fromUpstream" :key="v.id" class="sec-note">
          <strong class="mono">{{ upstreamLabel(v) }}</strong>
          {{ v.upstream?.commits.map((c) => c.subject).join("; ") }}
        </li>
      </ul>
      <p v-if="lastSync?.outcome === 'failed'" class="sec-note">Taking upstream: {{ lastSync.detail }}</p>
      <Logbook />
    </section>

    <BrainCard />

    <section class="sec">
      <BotCard />
    </section>

    <NewIdea v-model:open="creating" />
  </ESheet>
</template>

<style scoped>
.gap {
  margin-block-end: var(--e-gap-base);
}

.progress {
  gap: 0;
}

.upstream {
  display: grid;
  gap: var(--e-gap-tight);
  margin-block-end: var(--e-gap-snug);
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
