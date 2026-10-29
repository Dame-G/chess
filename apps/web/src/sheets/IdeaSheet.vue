<script setup lang="ts">
import { computed, ref, watch } from "vue";
import { RouterLink, useRoute, useRouter } from "vue-router";
import { ELink } from "@eos-ui/vue/components/ELink";
import { ENotice } from "@eos-ui/vue/components/ENotice";
import { ESkeleton } from "@eos-ui/vue/components/ESkeleton";
import { ApiError } from "../api/client";
import type { Seat } from "../api/types";
import { formatRelative, plural, plusMinus, resultOf, seatName } from "../format";
import IdeaActions from "../parts/IdeaActions.vue";
import { useIdeaActions } from "../parts/ideaActions";
import Notes from "../parts/Notes.vue";
import Term from "../parts/Term.vue";
import TrainView from "../parts/TrainView.vue";
import TrialEvidence from "../parts/TrialEvidence.vue";
import TuneView from "../parts/TuneView.vue";
import { useDameless } from "../stores/dameless";
import { useGames } from "../stores/games";
import { useIdeas } from "../stores/ideas";
import { useVersions } from "../stores/versions";
import { ideaTone, trialTone } from "../ui/badge";
import UiBadge from "../ui/UiBadge.vue";
import UiEmpty from "../ui/UiEmpty.vue";
import UiKeyValue from "../ui/UiKeyValue.vue";
import UiKv from "../ui/UiKv.vue";
import UiResult from "../ui/UiResult.vue";
import UiSection from "../ui/UiSection.vue";
import UiSheetPage from "../ui/UiSheetPage.vue";
import UiStat from "../ui/UiStat.vue";
import UiStats from "../ui/UiStats.vue";
import UiTable from "../ui/UiTable.vue";
import { decidingTrial, eloFigure, KIND_WORD, STAGE_WORD, STATUS_WORD, TRIAL_WORD } from "./idea";

/**
 * An idea: its verdict first (Elo against the base, what the trials said), and under it three tabs. Trials is the funnel
 * as a row of stages and, under it, the chosen stage's evidence: the gauge, Elo, the pairs, the clock it was played at,
 * and for a tune its trajectory. Notes is what it is for and who had it, and Games is every game its trials played.
 * Stop, Resume and Promote are in the header, Push the fork after promoting.
 */
const route = useRoute();
const router = useRouter();
const ideas = useIdeas();
const dameless = useDameless();
const versions = useVersions();
const games = useGames();

const tabs = [
  { id: "trials", label: "Trials" },
  { id: "notes", label: "Notes" },
  { id: "games", label: "Games" },
];
const tab = ref("trials");

const id = computed(() => Number(route.params["id"]));
const idea = computed(() => ideas.ideas[id.value]);
const missing = ref(false);
/** Why the idea could not be read, when it is not that there is no such idea. */
const failed = ref<string | null>(null);
const selected = ref<string | undefined>();

async function open() {
  missing.value = false;
  failed.value = null;
  selected.value = undefined;
  if (!dameless.harness) void dameless.loadHarness().catch(() => {});
  if (!ideas.ideas[id.value]) {
    try {
      await ideas.loadIdea(id.value);
    } catch (e) {
      if (e instanceof ApiError && e.status === 404) missing.value = true;
      else failed.value = e instanceof Error ? e.message : String(e);
      return;
    }
  }
  if (ideas.ideas[id.value]?.kind === "tune") void ideas.loadTune(id.value).catch(() => {});
}
watch(id, open, { immediate: true });

const trials = computed(() => idea.value?.trials ?? []);
/** The stage shown: the one chosen, else the one running, else the one that failed, else the last. */
const current = computed(
  () =>
    selected.value ??
    String(
      (
        trials.value.find((t) => t.status === "running") ??
        trials.value.find((t) => t.status === "failed") ??
        trials.value.at(-1)
      )?.id ?? "",
    ),
);
const trial = computed(() => trials.value.find((t) => String(t.id) === current.value));
const full = computed(() => (trial.value ? ideas.trials[trial.value.id] : undefined));
watch(
  trial,
  (t) => {
    if (t && t.stage !== "train" && !ideas.trials[t.id]) void ideas.loadTrial(id.value, t.id).catch(() => {});
  },
  { immediate: true },
);

/** The number that decides a stage: its Elo, else its pairs so far. */
const decider = (t: (typeof trials.value)[number]) =>
  t.elo ? plusMinus(t.elo) : t.pairs ? `${t.pairs.toLocaleString("en-GB")} pairs` : "";

const base = computed(() => (idea.value ? versions.name(idea.value.base) : undefined));
const change = computed(
  () => (idea.value?.version != null ? versions.name(idea.value.version) : undefined) ?? idea.value?.branch,
);
const tune = computed(() => ideas.tuneStates[id.value]);
const untrusted = computed(
  () => !!dameless.harness && !dameless.harness.trusted && dameless.harness.running == null && trials.value.length > 0,
);

const actions = useIdeaActions(() => idea.value!);

/**
 * The headline: the Elo that decided it (or is deciding it) with the pairs behind it, and the stage it is at. A speed,
 * tune or net idea has no Elo until games start, so there the stage and the verdict are the headline.
 */
const lead = computed(() => {
  const i = idea.value;
  if (!i) return null;
  const t = decidingTrial(i) ?? i.trials.find((x) => x.status === "running");
  const now =
    i.trials.find((x) => x.status === "running") ?? i.trials.find((x) => x.status === "failed") ?? i.trials.at(-1);
  return {
    elo: t?.elo ? eloFigure(t.elo) : null,
    pairs: t?.pairs ? plural(t.pairs, "pair") : undefined,
    stage: now ? STAGE_WORD[now.stage] : i.status === "queued" ? "Waiting" : null,
  };
});
const subtitle = computed(() => {
  const i = idea.value;
  return i
    ? `${KIND_WORD[i.kind]} · by ${i.author === "you" ? "you" : "Claude"} · ${formatRelative(i.created_at)}`
    : undefined;
});

// ---- Games: every game the idea's trials played, newest first (what the rail holds of them).

const trialStage = computed(() => new Map(trials.value.map((t) => [t.id as number, STAGE_WORD[t.stage]])));
interface GameRow {
  id: number;
  stage: string;
  white: string;
  black: string;
  result: ReturnType<typeof resultOf>;
  moves: number | null;
}
function name(seat: Seat): string {
  const version = seat.player === "dameless" ? versions.name(seat.version) : undefined;
  return version ?? seatName(seat);
}
const gameRows = computed<GameRow[]>(() =>
  games.list
    .filter((g) => g.source.type === "trial" && trialStage.value.has(g.source.trial))
    .map((g) => ({
      id: g.id,
      stage: trialStage.value.get((g.source as { trial: number }).trial) ?? "",
      white: name(g.white),
      black: name(g.black),
      result: g.status === "live" ? "*" : resultOf(g.outcome),
      moves: g.status === "live" ? null : Math.ceil(g.plies / 2),
    })),
);
const gameColumns = [
  { key: "white", label: "White" },
  { key: "black", label: "Black" },
  { key: "result", label: "Result", align: "center" as const },
  { key: "moves", label: "Moves", fig: true, hide: "sm" as const },
  { key: "stage", label: "Trial", hide: "md" as const },
];
const openGame = (row: GameRow) => void router.push({ path: "/", query: { game: String(row.id) } });
</script>

<template>
  <UiSheetPage
    v-model:tab="tab"
    :title="idea?.title ?? 'Idea'"
    :subtitle="subtitle"
    :tabs="idea ? tabs : undefined"
    wide
    :loading="!idea && !missing && !failed"
    :error="failed"
    @retry="open"
  >
    <template v-if="idea" #actions><IdeaActions :idea="idea" :actions="actions" /></template>

    <template v-if="idea && lead" #lead>
      <UiStats>
        <UiStat v-if="lead.elo" label="Elo" hint="elo" :value="lead.elo" unit="Elo" size="xl" :sub="lead.pairs" />
        <UiStat label="Stage" :value="lead.stage" />
        <UiStat label="Against" :value="base ?? 'The champion'" />
      </UiStats>
      <div class="status">
        <UiBadge :tone="ideaTone[idea.status]">{{ STATUS_WORD[idea.status] }}</UiBadge>
        <p v-if="idea.verdict" class="verdict">{{ idea.verdict }}</p>
      </div>
      <p v-if="actions.promoted && !actions.pushed" class="meta">
        Promoted on this machine. The fork's main has not been pushed to the remote.
      </p>
      <p v-if="actions.pushed" class="meta">Pushed at {{ actions.pushed }}.</p>
      <ENotice v-if="actions.problem" tone="danger" title="That did not work" role="alert">
        {{ actions.problem }}
      </ENotice>
    </template>

    <UiEmpty v-if="missing" title="That idea isn't here" body="It may have been removed, or the link is wrong.">
      <template #actions>
        <ELink :link-component="RouterLink" to="/dameless?tab=ideas">Back to the ideas</ELink>
      </template>
    </UiEmpty>

    <template v-if="idea" #tab-trials>
      <div>
        <ENotice v-if="untrusted" tone="warning" title="Results on this machine are not trusted yet">
          {{ dameless.harness?.summary }}
          <ELink :link-component="RouterLink" to="/settings?tab=machine">Check the harness in Settings</ELink>.
        </ENotice>
        <UiEmpty
          v-if="!trials.length"
          dense
          :title="
            idea.status === 'queued' ? 'It is waiting its turn. The first stage checks the bench.' : 'No trial ran.'
          "
        />
        <template v-else>
          <ol v-if="trials.length > 1" class="stages" aria-label="Stages">
            <li v-for="t in trials" :key="t.id">
              <button
                type="button"
                class="stage"
                :aria-pressed="current === String(t.id)"
                @click="selected = String(t.id)"
              >
                <span class="stage-name">{{ STAGE_WORD[t.stage] }}</span>
                <UiBadge :tone="trialTone[t.status]">{{ TRIAL_WORD[t.status] }}</UiBadge>
                <span class="stage-num">{{ decider(t) || "–" }}</span>
              </button>
            </li>
          </ol>
          <div v-if="trial" class="panel" aria-live="polite">
            <TuneView v-if="trial.stage === 'tune' && tune" :tune="tune" :running="trial.status === 'running'" />
            <TrainView v-else-if="trial.stage === 'train'" :idea="idea" :status="trial.status" />
            <ESkeleton v-else-if="!full && trial.stage !== 'tune'" :lines="4" label="Loading the stage" />
            <TrialEvidence v-else-if="full" :trial="full" />
            <p v-else-if="trial.detail" class="verdict">{{ trial.detail }}</p>
          </div>
          <p v-if="idea.kind === 'speed'" class="meta">
            A speed change must leave the <Term k="bench" /> exactly as it was; then the two versions race and no games
            are played.
          </p>
        </template>
      </div>
    </template>

    <template v-if="idea" #tab-notes>
      <div>
        <UiSection title="Hypothesis">
          <p class="prose">{{ idea.hypothesis || "No hypothesis was written." }}</p>
        </UiSection>
        <UiSection title="Details">
          <UiKeyValue>
            <UiKv label="Kind">{{ KIND_WORD[idea.kind] }}</UiKv>
            <UiKv v-if="idea.kind !== 'net'" label="Change" mono>{{ change }}</UiKv>
            <UiKv label="Against">
              <ELink :link-component="RouterLink" to="/dameless">{{ base ?? "The champion" }}</ELink>
            </UiKv>
            <UiKv label="By">{{ idea.author === "you" ? "You" : "Claude" }}</UiKv>
            <UiKv v-if="idea.tags.length" label="Tags">
              <span class="tags"
                ><UiBadge v-for="t in idea.tags" :key="t">{{ t }}</UiBadge></span
              >
            </UiKv>
          </UiKeyValue>
        </UiSection>
        <UiSection v-if="idea.notes" title="Notes"><Notes :text="idea.notes" /></UiSection>
      </div>
    </template>

    <template v-if="idea" #tab-games>
      <UiTable
        label="Games its trials played"
        :columns="gameColumns"
        :rows="gameRows"
        row-key="id"
        empty="Its trials have not played a game the rail holds yet."
        @select="openGame"
      >
        <template #cell-result="{ row }"><UiResult :result="row.result" /></template>
        <template #cell-moves="{ row }">{{ row.moves ?? "–" }}</template>
      </UiTable>
    </template>
  </UiSheetPage>
</template>

<style scoped>
.status {
  display: flex;
  flex-wrap: wrap;
  align-items: center;
  gap: var(--sp-2);
}

.verdict {
  margin: 0;
  font: var(--t-body-strong);
}

.meta {
  margin: 0;
  color: var(--c-ink-2);
  font: var(--t-meta);
}

.prose {
  max-inline-size: var(--w-measure);
  margin: 0;
}

.tags {
  display: flex;
  flex-wrap: wrap;
  gap: var(--sp-1);
}

/* The funnel is a row of equal cells, as many as it has stages: each says its name, its state and its number. */
.stages {
  display: grid;
  grid-auto-flow: column;
  grid-auto-columns: minmax(0, 1fr);
  gap: var(--sp-1);
  margin: 0;
  padding: 0;
  list-style: none;
}

.stage {
  position: relative;
  display: grid;
  justify-items: start;
  gap: var(--sp-1);
  inline-size: 100%;
  min-block-size: var(--h-row-2);
  padding: var(--sp-2) var(--sp-3);
  border: var(--e-stroke-hair) solid var(--c-line-strong);
  border-radius: var(--r-1);
  background: transparent;
  color: var(--c-ink);
  text-align: start;
  cursor: pointer;
}

.stage:hover {
  background: var(--c-raise);
}

.stage[aria-pressed="true"] {
  background: var(--c-sunk);
}

.stage[aria-pressed="true"]::after {
  content: "";
  position: absolute;
  inset: auto var(--sp-2) 0;
  block-size: var(--e-stroke-focus);
  background: var(--c-act);
}

.stage:focus-visible {
  outline: var(--focus-ring);
  outline-offset: calc(-1 * var(--e-stroke-focus));
}

.stage-name {
  max-inline-size: 100%;
  overflow: hidden;
  font: var(--t-body-strong);
  text-overflow: ellipsis;
  white-space: nowrap;
}

.stage-num {
  max-inline-size: 100%;
  overflow: hidden;
  color: var(--c-ink-2);
  font: var(--t-fig);
  text-overflow: ellipsis;
  white-space: nowrap;
}

.panel {
  display: grid;
  gap: var(--sp-3);
}

.stages + .panel {
  padding-block-start: var(--sp-4);
}

@media (width < 30rem) {
  /* Four stages do not fit side by side on a phone: two and two. */
  .stages {
    grid-auto-flow: row;
    grid-template-columns: repeat(2, minmax(0, 1fr));
  }
}
</style>
