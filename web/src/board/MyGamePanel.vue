<script setup lang="ts">
/** The inspector's head in one of your own games on Lichess: whose move it is, offers to answer, and what you can do. */
import { computed, onBeforeUnmount, ref, watch } from "vue";
import {
  PhArrowSquareOut,
  PhArrowUUpLeft,
  PhChartLine,
  PhCheck,
  PhFlag,
  PhHandshake,
  PhProhibit,
  PhX,
} from "@phosphor-icons/vue";
import { EButton } from "@eos-ui/vue/components/EButton";
import { EDialog } from "@eos-ui/vue/components/EDialog";
import { ESpinner } from "@eos-ui/vue/components/ESpinner";
import { EStamp } from "@eos-ui/vue/components/EStamp";
import { gameClock, speedName } from "../lichess";
import { offersOf } from "./lichessMe";
import { clockText, movesText, youOutcome, youSide, type Match } from "./matchText";

const props = defineProps<{
  match: Match;
  /** A request to Lichess is under way. */
  busy?: boolean;
  /** The opponent has left, and when you may claim (`at` is a `performance.now()` reading). */
  gone?: { gone: boolean; claimIn: number | null; at: number };
  /** Another game of yours keeps analysis off, so a finished game cannot be analysed yet. */
  locked?: boolean;
  /** The engine's analysis of the finished game is showing. */
  analysing?: boolean;
}>();
const emit = defineEmits<{
  abort: [];
  resign: [];
  draw: [];
  declineDraw: [];
  takeback: [];
  declineTakeback: [];
  claimVictory: [];
  claimDraw: [];
  analyse: [];
}>();

const me = computed(() => youSide(props.match) ?? props.match.lichess?.our_side ?? "white");
const them = computed(() => props.match[me.value === "white" ? "black" : "white"]);
const running = computed(() => props.match.status === "running");
const myTurn = computed(() => running.value && props.match.turn === me.value);
const plies = computed(() => props.match.moves.length);
/** Lichess lets a game be aborted until both sides have moved. */
const early = computed(() => plies.value < 2);
const offers = computed(() => offersOf(props.match, me.value));
const iMoved = computed(() => props.match.moves.some((m) => m.side === me.value));

const stamp = computed(() => (myTurn.value ? ({ label: "Your move", tone: "success" } as const) : null));
const sentence = computed(() => (running.value ? "" : youOutcome(props.match)));
const facts = computed(() => {
  const l = props.match.lichess;
  const parts = l ? [`${speedName(l.speed)} ${gameClock(props.match)}`, l.rated ? "rated" : "casual"] : [];
  parts.push(`You · ${me.value}`);
  if (plies.value) parts.push(movesText(plies.value));
  return parts.join(" · ");
});
const opening = computed(() => props.match.lichess?.opening ?? "");

// Resigning is not undone: ask first.
const resigning = ref(false);
function resign() {
  resigning.value = false;
  emit("resign");
}

// The countdown to claiming when the opponent has left.
const now = ref(performance.now());
let ticker: ReturnType<typeof setInterval> | undefined;
watch(
  () => props.gone?.gone && running.value,
  (on) => {
    clearInterval(ticker);
    if (!on) return;
    now.value = performance.now();
    ticker = setInterval(() => (now.value = performance.now()), 500);
  },
  { immediate: true },
);
onBeforeUnmount(() => clearInterval(ticker));
const leftMs = computed(() =>
  props.gone?.claimIn == null ? null : Math.max(0, props.gone.claimIn * 1000 - (now.value - props.gone.at)),
);
const away = computed(() => running.value && !!props.gone?.gone);
/** Claiming opens when the countdown ends. */
const waiting = computed(() => leftMs.value !== null && leftMs.value > 0);
</script>

<template>
  <header class="play">
    <p class="line">
      <EStamp v-if="stamp" size="sm" :tone="stamp.tone" :label="stamp.label" />
      <span v-else-if="running" class="wait" aria-live="polite">
        <ESpinner size="sm" :label="`${them.name} is thinking`" silent />
      </span>
      <strong v-if="sentence" class="sentence" aria-live="polite">{{ sentence }}</strong>
    </p>
    <p class="facts num">{{ facts }}</p>
    <p v-if="opening" class="opening">{{ opening }}</p>

    <template v-if="running">
      <div v-if="offers.theirDraw" class="ask" role="group" :aria-label="`${them.name} offers a draw`">
        <span>{{ them.name }} offers a draw</span>
        <EButton size="sm" variant="primary" :icon="PhCheck" :loading="busy" @click="emit('draw')">Accept</EButton>
        <EButton size="sm" variant="outline" :icon="PhX" :disabled="busy" @click="emit('declineDraw')">Decline</EButton>
      </div>
      <div v-if="offers.theirTakeback" class="ask" role="group" :aria-label="`${them.name} asks to take back a move`">
        <span>{{ them.name }} asks to take back a move</span>
        <EButton size="sm" variant="primary" :icon="PhCheck" :loading="busy" @click="emit('takeback')">
          Accept
        </EButton>
        <EButton size="sm" variant="outline" :icon="PhX" :disabled="busy" @click="emit('declineTakeback')">
          Decline
        </EButton>
      </div>
      <div v-if="away" class="ask" role="group" :aria-label="`${them.name} has left the game`">
        <span v-if="waiting">
          {{ them.name }} has left the game. You can claim in <span class="num">{{ clockText(leftMs ?? 0) }}</span>
        </span>
        <span v-else>{{ them.name }} has left the game</span>
        <EButton size="sm" variant="primary" :disabled="waiting" :loading="busy" @click="emit('claimVictory')">
          Claim victory
        </EButton>
        <EButton size="sm" variant="outline" :disabled="waiting || busy" @click="emit('claimDraw')">
          Call it a draw
        </EButton>
      </div>

      <div class="actions">
        <EButton v-if="early" variant="danger" size="sm" :icon="PhProhibit" :loading="busy" @click="emit('abort')">
          Abort
        </EButton>
        <template v-else>
          <EButton
            variant="outline"
            size="sm"
            :icon="PhHandshake"
            :disabled="offers.myDraw || offers.theirDraw || busy"
            @click="emit('draw')"
          >
            {{ offers.myDraw ? "Draw offered" : "Offer draw" }}
          </EButton>
          <EButton
            variant="outline"
            size="sm"
            :icon="PhArrowUUpLeft"
            :disabled="!iMoved || offers.myTakeback || offers.theirTakeback || busy"
            @click="emit('takeback')"
          >
            {{ offers.myTakeback ? "Takeback asked" : "Takeback" }}
          </EButton>
          <EButton variant="danger" size="sm" :icon="PhFlag" :disabled="busy" @click="resigning = true">
            Resign
          </EButton>
        </template>
      </div>
    </template>
    <div v-else class="actions">
      <EButton
        v-if="plies && !locked"
        :variant="analysing ? 'outline' : 'primary'"
        size="sm"
        :icon="PhChartLine"
        @click="emit('analyse')"
      >
        {{ analysing ? "Analysing" : "Analyse this game" }}
      </EButton>
      <EButton
        v-if="match.lichess"
        variant="outline"
        size="sm"
        :icon="PhArrowSquareOut"
        :href="match.lichess.url"
        external
      >
        Open on Lichess
      </EButton>
    </div>
  </header>
  <EDialog
    v-model:open="resigning"
    title="Resign this game?"
    description="You lose the game on Lichess. This cannot be undone."
    size="sm"
  >
    <template #actions>
      <EButton variant="danger" @click="resign">Resign</EButton>
      <EButton variant="outline" @click="resigning = false">Keep playing</EButton>
    </template>
  </EDialog>
</template>

<style scoped>
.play {
  display: grid;
  gap: var(--e-gap-snug);
  padding: var(--e-gap-base) var(--ws-pad);
}

.line {
  display: flex;
  flex-wrap: wrap;
  align-items: center;
  gap: var(--e-gap-snug);
  margin: 0;
}

.wait {
  display: inline-flex;
  align-items: center;
  gap: var(--e-gap-snug);
  font: var(--e-type-h6);
}

.sentence {
  font: var(--e-type-h6);
}

.facts,
.opening {
  margin: 0;
  color: var(--e-muted);
  font: var(--e-type-small);
}

.opening {
  color: var(--e-ink);
}

/* An offer to answer: the question, then its answers, in one row that wraps. */
.ask {
  display: flex;
  flex-wrap: wrap;
  align-items: center;
  gap: var(--e-gap-snug);
  padding-block: var(--e-gap-snug);
  border-block: var(--e-stroke-hair) solid color-mix(in oklab, var(--e-border) 40%, transparent);
  font: var(--e-type-small);
}

.ask > span {
  flex: 1 1 100%;
}

.actions {
  display: flex;
  flex-wrap: wrap;
  gap: var(--e-gap-snug);
}
</style>
