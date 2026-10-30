<script setup lang="ts">
import { computed, onMounted, ref, watch } from "vue";
import { RouterLink, useRoute, useRouter } from "vue-router";
import { PhLightbulb } from "@phosphor-icons/vue";
import { EButton } from "@eos-ui/vue/components/EButton";
import { ELink } from "@eos-ui/vue/components/ELink";
import { ENotice } from "@eos-ui/vue/components/ENotice";
import type { ChartSeries } from "../parts/chart";
import BotCard from "../parts/BotCard.vue";
import BrainCard from "../parts/BrainCard.vue";
import IdeaRow from "../parts/IdeaRow.vue";
import LevelsCard from "../parts/LevelsCard.vue";
import LineChart from "../parts/LineChart.vue";
import Logbook from "../parts/Logbook.vue";
import NewIdea from "../parts/NewIdea.vue";
import Term from "../parts/Term.vue";
import { formatElo, seatName } from "../format";
import { useConnections } from "../stores/connections";
import { useDameless } from "../stores/dameless";
import { useGames } from "../stores/games";
import { useIdeas } from "../stores/ideas";
import { useVersions } from "../stores/versions";
import UiEmpty from "../ui/UiEmpty.vue";
import UiLive from "../ui/UiLive.vue";
import UiSection from "../ui/UiSection.vue";
import UiSheetPage from "../ui/UiSheetPage.vue";
import UiStat from "../ui/UiStat.vue";
import UiStats from "../ui/UiStats.vue";

/**
 * Dameless: the champion and its strength over the versions and the levels (Strength), the ideas running and the
 * logbook of every one (Ideas), the network and the data it learns from (Brain), and its games on Lichess (Bot). New
 * idea is one dialog for every kind. Settings links to `/dameless#bot`, which opens the Bot tab.
 */
const dameless = useDameless();
const ideas = useIdeas();
const versions = useVersions();
const connections = useConnections();
const games = useGames();
const route = useRoute();
const router = useRouter();

const tabs = [
  { id: "strength", label: "Strength" },
  { id: "ideas", label: "Ideas" },
  { id: "brain", label: "Brain" },
  { id: "bot", label: "Bot" },
];
const tab = ref("strength");

const failed = ref<string | null>(null);
async function read() {
  failed.value = null;
  try {
    await dameless.load();
  } catch (e) {
    failed.value = e instanceof Error ? e.message : String(e);
  }
}
onMounted(() => void read());

/** `/dameless#bot` (Settings's Bot controls link) lands on the Bot tab; `?new=idea` on Ideas, with the dialog open. */
function fromAddress() {
  if (route.hash === "#bot") tab.value = "bot";
}
fromAddress();
watch(() => route.hash, fromAddress);

const overview = computed(() => dameless.overview);
const champion = computed(() => overview.value?.champion ?? versions.current);
const strength = computed(() => overview.value?.strength ?? []);
/** Versions that have been measured against each other: a line that never leaves zero has nothing to say yet. */
const measured = computed(() => strength.value.length > 1 && strength.value.some((p) => p.value !== 0));

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
  return `${p.label ?? ""}: ${formatElo(p.value, 1)} Elo over v1${margin}${p.measured ? ", measured" : ""}`;
});
/** The champion's Elo over v1 and its margin; nothing until a promoted version has been measured against v1. */
const over = computed(() => {
  const p = strength.value.at(-1);
  if (!p || !measured.value) return null;
  return { value: formatElo(p.value, 1), margin: p.high != null && p.low != null ? (p.high - p.low) / 2 : null };
});

/** The bot's live game: who it plays, and where to watch it. */
const playing = computed(() => {
  const id = connections.bot.playing[0];
  const g = id != null ? games.list.find((x) => x.id === id) : undefined;
  if (!g) return null;
  const them = g.white.player === "dameless" ? g.black : g.white;
  return { id, name: seatName(them), count: connections.bot.playing.length };
});
const botState = computed(() => {
  const c = connections.bot;
  if (!connections.loaded) return null;
  if (!c.configured) return "Not linked";
  if (c.bot?.online) return playing.value ? "Playing" : "Online";
  return !c.bot?.settings.version ? "No version" : "Offline";
});

/**
 * The last regression check when it failed: the champion is measurably worse than the one five promotions back. Said
 * on Strength and on Ideas, until a later check passes; nothing was reverted, so what to do is the person's call.
 */
const regression = computed(() => {
  const last = overview.value?.regressions[0];
  return last && !last.passed ? last : null;
});

/** Champions taken from upstream Reckless, newest first, and the last sync (shown when it failed). */
const fromUpstream = computed(() => (overview.value?.line ?? []).filter((v) => v.upstream));
const lastSync = computed(() => overview.value?.upstream ?? null);
function upstreamLabel(v: (typeof fromUpstream.value)[number]): string {
  const n = v.upstream?.commits.length ?? 0;
  return `${v.name} · from upstream · ${n} commit${n === 1 ? "" : "s"}`;
}

const inProgress = computed(() => [...ideas.running, ...ideas.queued]);

// New idea: opened from the button, or by `/dameless?new=idea` (the command bar's "new idea").
const creating = ref(route.query["new"] === "idea");
if (creating.value) tab.value = "ideas";
watch(
  () => route.query["new"],
  (v) => {
    if (v === "idea") {
      tab.value = "ideas";
      creating.value = true;
    }
  },
);
watch(creating, (open) => {
  if (!open && route.query["new"] === "idea") void router.replace({ path: "/dameless" });
});
</script>

<template>
  <UiSheetPage
    v-model:tab="tab"
    title="Dameless"
    :tabs="tabs"
    wide
    :loading="!overview && !failed"
    :error="failed"
    @retry="read"
  >
    <template #lead>
      <UiStats :columns="3">
        <UiStat label="Champion" hint="champion" :value="champion?.name ?? null" size="xl" />
        <UiStat
          label="Over v1"
          hint="elo"
          :value="over?.value ?? null"
          unit="Elo"
          :sub="over ? (over.margin != null ? `±${over.margin.toFixed(1)}` : undefined) : 'Not measured yet'"
          none="Measured once a promoted version plays v1"
        />
        <UiStat label="Bot" :value="botState">
          <template v-if="playing" #sub>
            <RouterLink :to="{ path: '/', query: { game: String(playing.id) } }" class="playing">
              <UiLive label="Live" /> {{ playing.name
              }}<template v-if="playing.count > 1"> and {{ playing.count - 1 }} more</template>
            </RouterLink>
          </template>
        </UiStat>
      </UiStats>
    </template>

    <template #tab-strength>
      <div>
        <ENotice v-if="regression" tone="danger" title="Dameless got weaker" role="alert">
          {{ regression.summary }}.
          <ELink :link-component="RouterLink" :to="`/dameless/ideas/${regression.idea}`">Read the check</ELink>.
        </ENotice>
        <UiSection title="Strength">
          <template #aside>
            <ELink :link-component="RouterLink" :to="{ path: '/compare', query: { players: 'you,dameless' } }">
              Compare with you
            </ELink>
          </template>
          <LineChart
            v-if="measured"
            label="Dameless's strength: cumulative Elo over each champion version"
            :x="x"
            :series="series"
            :height="170"
            :x-format="(v) => (Number.isInteger(v) ? `v${v}` : '')"
            :format="(v) => formatElo(v, 0)"
            @point="hover = $event"
          >
            <span v-if="hover != null" class="mono">{{ readout }}</span>
          </LineChart>
          <UiEmpty v-else dense title="The line starts once a promoted version has been measured against v1." />
          <p v-if="strength.some((p) => p.measured)" class="muted">
            Some versions are placed by a <Term k="regression" text="regression check" />: games against the champion
            five promotions earlier, rather than each promotion's trial added up.
          </p>
        </UiSection>

        <UiSection v-if="fromUpstream.length || lastSync?.outcome === 'failed'" title="From upstream">
          <ul class="upstream">
            <li v-for="v in fromUpstream" :key="v.id">
              <strong class="mono">{{ upstreamLabel(v) }}</strong>
              {{ v.upstream?.commits.map((c) => c.subject).join("; ") }}
            </li>
          </ul>
          <p v-if="lastSync?.outcome === 'failed'" class="muted">Taking upstream: {{ lastSync.detail }}</p>
        </UiSection>

        <LevelsCard />
      </div>
    </template>

    <template #tab-ideas>
      <div>
        <UiEmpty
          v-if="ideas.loaded && !ideas.list.length"
          :icon="PhLightbulb"
          title="No ideas yet"
          body="An idea is a change to try against the champion."
        >
          <template #actions><EButton @click="creating = true">New idea</EButton></template>
        </UiEmpty>
        <template v-else>
          <ENotice v-if="regression" tone="danger" title="Dameless got weaker" role="alert">
            {{ regression.summary }}.
            <ELink :link-component="RouterLink" :to="`/dameless/ideas/${regression.idea}`">Read the check</ELink>.
          </ENotice>
          <UiSection title="In progress">
            <template #aside><EButton size="sm" @click="creating = true">New idea</EButton></template>
            <ul v-if="inProgress.length" class="rows">
              <li v-for="i in inProgress" :key="i.id"><IdeaRow :idea="i" /></li>
            </ul>
            <UiEmpty v-else dense :title="ideas.loaded ? 'Nothing is being tried.' : 'Loading the ideas…'" />
          </UiSection>
          <Logbook />
        </template>
        <NewIdea v-model:open="creating" />
      </div>
    </template>

    <template #tab-brain><BrainCard /></template>
    <template #tab-bot><BotCard /></template>
  </UiSheetPage>
</template>

<style scoped>
.rows {
  display: grid;
  gap: var(--sp-1);
  margin: 0;
  padding: 0;
  list-style: none;
}

.upstream {
  display: grid;
  gap: var(--sp-2);
  margin: 0;
  padding: 0;
  color: var(--c-ink-2);
  font: var(--t-meta);
  list-style: none;
}

.muted {
  margin: 0;
  color: var(--c-ink-2);
  font: var(--t-meta);
}

.playing {
  display: inline-flex;
  align-items: center;
  gap: var(--sp-1);
  color: inherit;
  text-decoration: underline;
  text-decoration-color: var(--c-line-strong);
  text-underline-offset: 0.2em;
}
</style>
