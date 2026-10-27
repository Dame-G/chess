<script setup lang="ts">
import { computed } from "vue";
import { RouterLink } from "vue-router";
import { ELink } from "@eos-ui/vue/components/ELink";
import type { Seat, Trial, TrialId } from "../api/types";
import { plural, plusMinus, resultText, seatName, trialClock } from "../format";
import { pairsOf, STAGE_WORD } from "../sheets/idea";
import { useGames } from "../stores/games";
import { useIdeas } from "../stores/ideas";
import { useVersions } from "../stores/versions";
import LineChart from "./LineChart.vue";
import LlrGauge from "./LlrGauge.vue";
import PentaBar from "./PentaBar.vue";
import Term from "./Term.vue";

/**
 * One trial's evidence: the sentence that says what happened, then (for a test with games) the evidence gauge between
 * its lines, Elo and nElo with their margins, the pairs, wins, draws and losses, the clock it was played at, the
 * evidence over the pairs played, and its games, each opening on the board.
 */
const props = defineProps<{ trial: Trial }>();

const ideas = useIdeas();
const games = useGames();
const versions = useVersions();

const pairs = computed(() => pairsOf(props.trial.penta));
const hasGames = computed(() => pairs.value > 0 || props.trial.wdl.some((n) => n > 0));
const history = computed(() => ideas.llrHistory[props.trial.id as TrialId] ?? []);
const scaleText = computed(() => {
  const t = props.trial;
  if (!t.played || !t.scaling) return t.time ? `Clock ${trialClock(t.time)}` : "";
  const written = t.time ? `Clock ${trialClock(t.time)}, played` : "Played";
  return `${written} at ${trialClock(t.played)} (×${t.scaling.scale.toFixed(2)})`;
});
const bounds = computed(() => props.trial.bounds);

const trialGames = computed(() =>
  games.list.filter((g) => g.source.type === "trial" && g.source.trial === props.trial.id).slice(0, 12),
);
const total = computed(
  () => games.list.filter((g) => g.source.type === "trial" && g.source.trial === props.trial.id).length,
);
/** A trial pits two versions of Dameless: each is named by its version (`deeper-lmr`, `v14`), not just "Dameless". */
function name(seat: Seat): string {
  const version = seat.player === "dameless" ? versions.name(seat.version) : undefined;
  return version ?? seatName(seat);
}
</script>

<template>
  <div class="evidence">
    <p v-if="trial.detail" class="detail" :class="{ 'detail--soft': hasGames }">{{ trial.detail }}</p>
    <p v-else class="sec-note">{{ STAGE_WORD[trial.stage] }}: nothing to report yet.</p>

    <template v-if="hasGames">
      <LlrGauge v-if="trial.llr" :llr="trial.llr" />

      <dl class="figs figs--small">
        <div v-if="trial.elo" class="fig">
          <dt><Term k="elo" /></dt>
          <dd>{{ plusMinus(trial.elo) }}</dd>
        </div>
        <div v-if="trial.nelo" class="fig">
          <dt><Term k="nelo" /></dt>
          <dd>{{ plusMinus(trial.nelo) }}</dd>
        </div>
        <div class="fig">
          <dt><Term k="pairs" /></dt>
          <dd>{{ pairs.toLocaleString("en-GB") }}</dd>
        </div>
        <div class="fig fig--wide">
          <dt>Won · drawn · lost</dt>
          <dd>{{ trial.wdl.map((n) => n.toLocaleString("en-GB")).join(" · ") }}</dd>
        </div>
        <div v-if="bounds" class="fig">
          <dt>Goal in <Term k="nelo" /> (<Term k="sprt" />)</dt>
          <dd>[{{ bounds.map((b) => String(b).replace("-", "−")).join(", ") }}]</dd>
        </div>
      </dl>

      <PentaBar :penta="trial.penta" />

      <p v-if="scaleText" class="sec-note">
        <Term k="scaling" :text="scaleText" />
      </p>

      <div v-if="history.length > 2 && trial.llr" class="history">
        <h4 class="part-title">Evidence over the pairs played</h4>
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
      </div>

      <div v-if="trialGames.length" class="games">
        <h4 class="part-title">Its games</h4>
        <ul class="plain">
          <li v-for="g in trialGames" :key="g.id" class="game">
            <ELink :link-component="RouterLink" :to="{ path: '/', query: { game: String(g.id) } }">
              {{ name(g.white) }} – {{ name(g.black) }}
            </ELink>
            <span class="mono res">{{ resultText(g.outcome) }}</span>
            <span class="sec-note">{{
              g.status === "live" ? "playing now" : plural(Math.ceil(g.plies / 2), "move")
            }}</span>
          </li>
        </ul>
        <p v-if="total > trialGames.length" class="sec-note">
          The newest {{ trialGames.length }} of {{ total }}. All of them are in the Games list under Trials.
        </p>
      </div>
    </template>
  </div>
</template>

<style scoped>
.evidence {
  display: grid;
  gap: var(--e-gap-base);
  min-width: 0;
}

.detail {
  margin: 0;
  font-weight: 600;
}

/* With games under it, the sentence only restates the figures: quieter than they are. */
.detail--soft {
  color: var(--e-muted);
  font: var(--e-type-small);
}

/* Three counts side by side stay on one line, so they take two columns, or the row when it is one column wide. */
.fig--wide {
  grid-column: span 2;
}

.fig--wide dd {
  white-space: nowrap;
}

@media (width < 30rem) {
  .fig--wide {
    grid-column: 1 / -1;
  }
}

.history,
.games {
  display: grid;
  gap: var(--e-gap-tight);
}

.game {
  display: grid;
  grid-template-columns: minmax(0, 1fr) 3.5rem 7rem;
  align-items: baseline;
  gap: var(--e-gap-snug);
  padding-block: 0.1875rem;
  border-block-end: var(--line);
}

.res {
  text-align: center;
}
</style>
