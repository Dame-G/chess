<script setup lang="ts">
import { computed, ref } from "vue";
import { PhArrowCounterClockwise, PhArrowSquareOut, PhFlag, PhStop, PhSwap } from "@phosphor-icons/vue";
import { EButton } from "@eos-ui/vue/components/EButton";
import { EDialog } from "@eos-ui/vue/components/EDialog";
import { EStamp } from "@eos-ui/vue/components/EStamp";
import { gameClock, speedName } from "../lichess";
import { matchTimeText, movesText, outcomeText, type Match } from "./matchText";

const props = defineProps<{
  match: Match;
  /** Whether the board shows the latest move. */
  attached: boolean;
  /** A request to the runner is under way. */
  busy?: boolean;
}>();
const emit = defineEmits<{ stop: []; rematch: []; swap: [] }>();

const lichess = computed(() => props.match.lichess ?? undefined);
// Resigning a game someone else is playing against is not undone: ask first.
const resigning = ref(false);
function resign() {
  resigning.value = false;
  emit("stop");
}
/** Before both sides have moved the game is aborted, not lost. */
const early = computed(() => props.match.moves.length < 2);

const running = computed(() => props.match.status === "running");
/** Only a game still on has a badge: once it is over the sentence says how it ended. */
const stamp = computed(() =>
  running.value ? ({ label: props.attached ? "Live" : "Behind", tone: "info" } as const) : null,
);
/** Live, the stamp says it all; behind the latest move it says what the board is showing. */
const sentence = computed(() => {
  if (lichess.value && !running.value) return outcomeText(props.match);
  if (props.match.status === "aborted") return "No result";
  if (!running.value) return outcomeText(props.match, { white: props.match.white.name, black: props.match.black.name });
  return props.attached ? "" : "Not showing the latest move";
});
const facts = computed(() => {
  const m = props.match;
  const l = m.lichess;
  const parts = l ? [`${speedName(l.speed)} ${gameClock(m)}`, l.rated ? "rated" : "casual"] : [matchTimeText(m.time)];
  parts.push(movesText(m.moves.length));
  if (m.book) parts.push("book opening");
  return parts.join(" · ");
});
</script>

<template>
  <header class="status">
    <p class="line">
      <EStamp v-if="stamp" size="sm" :tone="stamp.tone" :label="stamp.label" />
      <strong v-if="sentence" class="sentence" aria-live="polite">{{ sentence }}</strong>
      <span v-else class="facts num">{{ facts }}</span>
    </p>
    <p v-if="sentence" class="facts num">{{ facts }}</p>
    <div class="actions">
      <template v-if="lichess">
        <EButton v-if="running" variant="danger" size="sm" :icon="PhFlag" :loading="busy" @click="resigning = true">
          {{ early ? "Abort game" : "Resign" }}
        </EButton>
        <EButton variant="outline" size="sm" :icon="PhArrowSquareOut" :href="lichess.url" external>
          View on Lichess
        </EButton>
      </template>
      <template v-else>
        <EButton v-if="running" variant="danger" size="sm" :icon="PhStop" :loading="busy" @click="emit('stop')">
          Stop match
        </EButton>
        <template v-else>
          <EButton variant="outline" size="sm" :icon="PhArrowCounterClockwise" :loading="busy" @click="emit('rematch')">
            Rematch
          </EButton>
          <EButton variant="outline" size="sm" :icon="PhSwap" :loading="busy" @click="emit('swap')">
            Swap colours
          </EButton>
        </template>
      </template>
    </div>
  </header>
  <EDialog
    v-model:open="resigning"
    :title="early ? 'Abort this game?' : 'Resign this game?'"
    :description="
      early
        ? 'Nobody has made a move yet, so the game is cancelled and no rating changes.'
        : 'The bot gives up the game now and it counts as a loss on Lichess. This cannot be undone.'
    "
    size="sm"
  >
    <template #actions>
      <EButton variant="danger" @click="resign">{{ early ? "Abort game" : "Resign" }}</EButton>
      <EButton variant="outline" @click="resigning = false">Keep playing</EButton>
    </template>
  </EDialog>
</template>

<style scoped>
.status {
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
