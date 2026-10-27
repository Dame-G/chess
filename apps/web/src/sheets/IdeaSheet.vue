<script setup lang="ts">
import { computed, ref, watch } from "vue";
import { RouterLink, useRoute } from "vue-router";
import { ESheet } from "@eos-ui/vue/components/ESheet";
import { EButton } from "@eos-ui/vue/components/EButton";
import { EEmptyState } from "@eos-ui/vue/components/EEmptyState";
import { ELink } from "@eos-ui/vue/components/ELink";
import { ESkeleton } from "@eos-ui/vue/components/ESkeleton";
import { EStamp } from "@eos-ui/vue/components/EStamp";
import { ETag } from "@eos-ui/vue/components/ETag";
import { ago, plusMinus } from "../format";
import IdeaActions from "../parts/IdeaActions.vue";
import Notes from "../parts/Notes.vue";
import Term from "../parts/Term.vue";
import TrainView from "../parts/TrainView.vue";
import TrialEvidence from "../parts/TrialEvidence.vue";
import TuneView from "../parts/TuneView.vue";
import { useIdeas } from "../stores/ideas";
import { useVersions } from "../stores/versions";
import { clockOf, KIND_WORD, STAGE_WORD, STATUS_TONE, STATUS_WORD, TRIAL_TONE } from "./idea";
import SheetBack from "./SheetBack.vue";
import { useSheet } from "./useSheet";

/**
 * An idea: what it is for, how it is going or ended, the funnel as a row of stages each with its status and deciding
 * number, and under it the chosen stage's evidence: the gauge, Elo, the pairs, the clock it was played at, its games,
 * and for a tune its trajectory. Stop, Resume and Promote are in the head, Push the fork after promoting.
 */
const sheet = useSheet();
const route = useRoute();
const ideas = useIdeas();
const versions = useVersions();

const id = computed(() => Number(route.params["id"]));
const idea = computed(() => ideas.ideas[id.value]);
const missing = ref(false);
const selected = ref<string | undefined>();

async function open() {
  missing.value = false;
  selected.value = undefined;
  if (!ideas.ideas[id.value]) {
    try {
      await ideas.loadIdea(id.value);
    } catch {
      missing.value = true;
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

const decider = (t: (typeof trials.value)[number]) =>
  t.elo ? plusMinus(t.elo) : t.pairs ? `${t.pairs.toLocaleString("en-GB")} pairs` : "";
const trialClockText = (t: NonNullable<typeof trial.value>) => clockOf(t, ideas.trials[t.id]);

const base = computed(() => (idea.value ? versions.name(idea.value.base) : undefined));
const change = computed(
  () => (idea.value?.version != null ? versions.name(idea.value.version) : undefined) ?? idea.value?.branch,
);
const tune = computed(() => ideas.tuneStates[id.value]);
</script>

<template>
  <ESheet v-bind="sheet.bind" :title="idea?.title ?? 'Idea'" :description="idea?.hypothesis">
    <template #leading><SheetBack :show="sheet.canGoBack" @back="sheet.back" /></template>
    <EEmptyState v-if="missing" title="No such idea">It may have been removed, or the link is wrong.</EEmptyState>
    <div v-else-if="!idea" aria-busy="true"><ESkeleton :lines="5" label="Loading the idea" /></div>
    <template v-else>
      <section class="sec" aria-label="The idea">
        <div class="meta">
          <EStamp :tone="STATUS_TONE[idea.status]" :label="STATUS_WORD[idea.status]" />
          <span class="sec-note">
            {{ idea.author === "you" ? "You" : "Claude" }} · {{ KIND_WORD[idea.kind] }} ·
            {{ idea.kind === "net" ? "trains a network, tried" : "tries" }}
            <span v-if="idea.kind !== 'net'" class="mono">{{ change }}</span> against
            <ELink :link-component="RouterLink" to="/dameless">{{ base ?? "the champion" }}</ELink> ·
            {{ ago(idea.created_at) }}
          </span>
        </div>
        <p v-if="idea.verdict" class="verdict">{{ idea.verdict }}</p>
        <p v-if="idea.tags.length" class="tags"><ETag v-for="t in idea.tags" :key="t" :label="t" size="sm" /></p>
        <IdeaActions :idea="idea" />
      </section>

      <section v-if="idea.notes" class="sec" aria-labelledby="i-notes">
        <h2 id="i-notes" class="part-title">Notes</h2>
        <Notes :text="idea.notes" />
      </section>

      <section class="sec" aria-labelledby="i-funnel">
        <div class="sec-head">
          <h2 id="i-funnel" class="part-title">The trials</h2>
          <p class="sec-note">Each stage must pass before the next one runs.</p>
        </div>
        <p v-if="!trials.length" class="sec-note">
          {{ idea.status === "queued" ? "It is waiting its turn. The first stage checks the bench." : "No trial ran." }}
        </p>
        <ol v-else class="funnel" aria-label="Stages">
          <li v-for="t in trials" :key="t.id" class="funnel-item">
            <EButton
              variant="quiet"
              class="stage"
              :aria-pressed="current === String(t.id)"
              @click="selected = String(t.id)"
            >
              <span class="stage-name">{{ STAGE_WORD[t.stage] }}</span>
              <span class="stage-line">
                <EStamp size="sm" :tone="TRIAL_TONE[t.status]" :label="t.status" />
                <span class="stage-num mono">{{ decider(t) }}</span>
              </span>
            </EButton>
          </li>
        </ol>
        <div v-if="trial" class="panel" aria-live="polite">
          <h3 class="panel-title">
            {{ STAGE_WORD[trial.stage] }}
            <span v-if="trialClockText(trial)" class="mono sec-note"> · {{ trialClockText(trial) }}</span>
          </h3>
          <TuneView v-if="trial.stage === 'tune' && tune" :tune="tune" :running="trial.status === 'running'" />
          <TrainView v-if="trial.stage === 'train'" :idea="idea" :status="trial.status" />
          <ESkeleton v-else-if="!full && trial.stage !== 'tune'" :lines="4" label="Loading the stage" />
          <TrialEvidence v-else-if="full" :trial="full" />
          <p v-else-if="trial.detail" class="verdict">{{ trial.detail }}</p>
        </div>
        <p v-if="idea.kind === 'speed'" class="sec-note">
          A speed change must leave the <Term k="bench" /> exactly as it was; then the two builds race and no games are
          played.
        </p>
      </section>
    </template>
  </ESheet>
</template>

<style scoped>
.meta {
  display: flex;
  flex-wrap: wrap;
  align-items: center;
  gap: var(--e-gap-snug);
}

.verdict {
  margin: 0;
  font-weight: 600;
}

.tags {
  display: flex;
  flex-wrap: wrap;
  gap: var(--e-gap-tight);
  margin: 0;
}

.funnel {
  display: flex;
  flex-wrap: wrap;
  gap: var(--e-gap-tight);
  margin: 0;
  padding: 0;
  list-style: none;
}

.funnel-item {
  display: flex;
  align-items: center;
}

.funnel-item:not(:last-child)::after {
  margin-inline: 0.125rem;
  color: var(--e-muted);
  content: "›";
}

.stage {
  display: grid;
  justify-items: start;
  gap: 0.25rem;
  block-size: auto;
  padding-block: 0.375rem;
  border: var(--line);
  text-align: start;
}

/* The chosen stage is marked by its border and a soft wash; its words stay in ink, which the button's pressed colour
   would turn white on the light wash. */
.stage[aria-pressed="true"] {
  border-color: var(--e-primary);
  background: var(--e-primary-soft);
  color: var(--e-ink);
}

.stage[aria-pressed="true"] .stage-name,
.stage[aria-pressed="true"] .stage-num {
  color: var(--e-ink);
}

.stage-name {
  font-weight: 600;
}

.stage-line {
  display: inline-flex;
  align-items: center;
  gap: var(--e-gap-snug);
}

.stage-num {
  color: var(--e-muted);
  font-size: 0.8125rem;
}

.panel {
  display: grid;
  gap: var(--e-gap-snug);
  padding-block-start: var(--e-gap-base);
}

.panel-title {
  margin: 0;
  font: var(--e-type-h5, 600 1rem/1.2 var(--e-font-sans));
}
</style>
