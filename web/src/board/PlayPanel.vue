<script setup lang="ts">
/** The inspector's head in a game you play: whose move it is, and what you can do about it. */
import { computed, ref } from "vue";
import { PhArrowCounterClockwise, PhArrowUUpLeft, PhChartLine, PhFlag, PhHandshake, PhSwap } from "@phosphor-icons/vue";
import { EButton } from "@eos-ui/vue/components/EButton";
import { EDialog } from "@eos-ui/vue/components/EDialog";
import { ESpinner } from "@eos-ui/vue/components/ESpinner";
import { EStamp } from "@eos-ui/vue/components/EStamp";
import { matchTimeText, youOutcome, youSide, type Match } from "./matchText";

const props = defineProps<{
  match: Match;
  /** Whether the board shows the latest move. */
  attached: boolean;
  /** A request to the runner is under way. */
  busy?: boolean;
  /** The engine's analysis of the finished game is showing. */
  analysing?: boolean;
}>();
const emit = defineEmits<{
  resign: [];
  takeback: [];
  draw: [];
  analyse: [];
  rematch: [];
  swap: [];
  fresh: [];
}>();

const me = computed(() => youSide(props.match)!);
const them = computed(() => props.match[me.value === "white" ? "black" : "white"]);
const running = computed(() => props.match.status === "running");
const myTurn = computed(() => running.value && props.match.turn === me.value);
const timed = computed(() => props.match.time.type === "clock");
const plies = computed(() => props.match.moves.length);

const stamp = computed(() => {
  if (running.value) return myTurn.value ? ({ label: "Your move", tone: "success" } as const) : null;
  return { label: props.match.status === "aborted" ? "Cancelled" : "Final", tone: "neutral" } as const;
});
const sentence = computed(() => (running.value ? "" : youOutcome(props.match)));
/** The clock as you chose it, in minutes and seconds: `5+3`. */
const timeText = computed(() => {
  const t = props.match.time;
  return t.type === "clock" ? `${t.seconds / 60}+${t.increment}` : matchTimeText(t);
});
const facts = computed(() => {
  const moves = Math.ceil(plies.value / 2);
  return [`You · ${me.value}`, `${them.value.name} ${them.value.commit}`.trim(), timeText.value]
    .concat(moves ? [`${moves} move${moves === 1 ? "" : "s"}`] : [])
    .join(" · ");
});

// Resigning is not undone: ask first (before both sides have moved, the game is just cancelled).
const resigning = ref(false);
const early = computed(() => plies.value < 2);
function resign() {
  resigning.value = false;
  emit("resign");
}
</script>

<template>
  <header class="play">
    <p class="line">
      <EStamp v-if="stamp" size="sm" :tone="stamp.tone" :label="stamp.label" />
      <span v-else class="wait" aria-live="polite">
        <ESpinner size="sm" :label="`${them.name} is thinking`" silent />
      </span>
      <strong v-if="sentence" class="sentence" aria-live="polite">{{ sentence }}</strong>
    </p>
    <p class="facts num">{{ facts }}</p>

    <div v-if="running" class="actions">
      <EButton variant="danger" size="sm" :icon="PhFlag" :loading="busy" @click="resigning = true">
        {{ early ? "Cancel game" : "Resign" }}
      </EButton>
      <EButton
        v-if="plies >= 2"
        variant="outline"
        size="sm"
        :icon="PhHandshake"
        :disabled="!myTurn"
        @click="emit('draw')"
      >
        Offer draw
      </EButton>
      <EButton
        v-if="!timed && plies >= 2"
        variant="outline"
        size="sm"
        :icon="PhArrowUUpLeft"
        :disabled="!myTurn"
        @click="emit('takeback')"
      >
        Take back
      </EButton>
    </div>
    <div v-else class="actions">
      <EButton
        v-if="plies"
        :variant="analysing ? 'outline' : 'primary'"
        size="sm"
        :icon="PhChartLine"
        @click="emit('analyse')"
      >
        {{ analysing ? "Analysing" : "Analyse this game" }}
      </EButton>
      <EButton variant="outline" size="sm" :icon="PhArrowCounterClockwise" :loading="busy" @click="emit('rematch')">
        Rematch
      </EButton>
      <EButton variant="outline" size="sm" :icon="PhSwap" :loading="busy" @click="emit('swap')"> Swap colours </EButton>
      <EButton variant="quiet" size="sm" @click="emit('fresh')">New game</EButton>
    </div>
  </header>
  <EDialog
    v-model:open="resigning"
    :title="early ? 'Cancel this game?' : 'Resign this game?'"
    :description="early ? 'Nobody has moved yet, so it is not counted.' : 'You lose the game. This cannot be undone.'"
    size="sm"
  >
    <template #actions>
      <EButton variant="danger" @click="resign">{{ early ? "Cancel game" : "Resign" }}</EButton>
      <EButton variant="quiet" @click="resigning = false">Keep playing</EButton>
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

.facts {
  margin: 0;
  color: var(--e-muted);
  font: var(--e-type-small);
}

.actions {
  display: flex;
  flex-wrap: wrap;
  gap: var(--e-gap-snug);
}
</style>
