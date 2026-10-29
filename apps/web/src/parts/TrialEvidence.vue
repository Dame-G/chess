<script setup lang="ts">
import { computed } from "vue";
import type { Trial, TrialId } from "../api/types";
import { formatNumber, trialClock } from "../format";
import { eloFigure, pairsOf, STAGE_WORD } from "../sheets/idea";
import { useIdeas } from "../stores/ideas";
import UiSection from "../ui/UiSection.vue";
import UiStat from "../ui/UiStat.vue";
import UiStats from "../ui/UiStats.vue";
import LineChart from "./LineChart.vue";
import LlrGauge from "./LlrGauge.vue";
import PentaBar from "./PentaBar.vue";
import Term from "./Term.vue";

/**
 * One trial's evidence: the sentence that says what happened, then (for a trial with games) the evidence gauge
 * between its lines, Elo and nElo with their margins, the pairs, wins, draws and losses, the pentanomial bar, the clock
 * it was played at and the evidence over the pairs played. Every figure sits in a fixed column and is mono, so a live
 * trial updates in place without anything moving. Its games are on the idea's Games tab.
 */
const props = defineProps<{ trial: Trial }>();

const ideas = useIdeas();

const pairs = computed(() => pairsOf(props.trial.penta));
const hasGames = computed(() => pairs.value > 0 || props.trial.wdl.some((n) => n > 0));
const history = computed(() => ideas.llrHistory[props.trial.id as TrialId] ?? []);
const scaleText = computed(() => {
  const t = props.trial;
  if (!t.played || !t.scaling) return t.time ? `Clock ${trialClock(t.time)}` : "";
  const written = t.time ? `Clock ${trialClock(t.time)}, played` : "Played";
  return `${written} at ${trialClock(t.played)} (×${t.scaling.scale.toFixed(2)})`;
});
const goal = computed(() => props.trial.bounds?.map((b) => String(b).replace("-", "−")).join(", "));
const wdl = computed(() => props.trial.wdl.map((n) => formatNumber(n)));
</script>

<template>
  <div class="evidence">
    <p v-if="trial.detail" class="detail" :class="{ 'detail--soft': hasGames }">{{ trial.detail }}</p>
    <p v-else class="none">{{ STAGE_WORD[trial.stage] }}: nothing to report yet.</p>

    <template v-if="hasGames">
      <LlrGauge v-if="trial.llr" :llr="trial.llr" />

      <UiStats>
        <UiStat label="Elo" hint="elo" :value="trial.elo ? eloFigure(trial.elo) : null" />
        <UiStat label="nElo" hint="nelo" :value="trial.nelo ? eloFigure(trial.nelo) : null" />
        <UiStat label="Pairs" hint="pairs" :value="pairs" />
      </UiStats>

      <UiStats dense>
        <UiStat label="Won" :value="wdl[0]" />
        <UiStat label="Drawn" :value="wdl[1]" />
        <UiStat label="Lost" :value="wdl[2]" />
        <UiStat v-if="goal" label="Goal, nElo" hint="sprt" :value="`[${goal}]`" />
      </UiStats>

      <PentaBar :penta="trial.penta" />

      <p v-if="scaleText" class="clock"><Term k="scaling" :text="scaleText" /></p>

      <UiSection v-if="history.length > 2 && trial.llr" title="Evidence over the pairs played" :level="2">
        <LineChart
          label="The evidence for this change over the game pairs played"
          :x="history.map((h) => h[0])"
          :series="[{ label: 'Evidence', values: history.map((h) => h[1]), color: '--e-ink', width: 1.75 }]"
          :lines="[
            { value: trial.llr.upper, color: '--e-success' },
            { value: trial.llr.lower, color: '--e-danger' },
          ]"
          :range="[trial.llr.lower - 0.4, trial.llr.upper + 0.4]"
          :height="120"
          :x-format="(v) => String(Math.round(v))"
          :format="(v) => v.toFixed(1)"
        />
      </UiSection>
    </template>
  </div>
</template>

<style scoped>
.evidence {
  display: grid;
  gap: var(--sp-4);
  min-width: 0;
}

.detail {
  margin: 0;
  font: var(--t-body-strong);
}

/* With games under it, the sentence only restates the figures: quieter than they are. */
.detail--soft {
  color: var(--c-ink-2);
  font: var(--t-meta);
}

.none,
.clock {
  margin: 0;
  color: var(--c-ink-2);
  font: var(--t-meta);
}
</style>
