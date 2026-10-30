<script setup lang="ts">
import { computed } from "vue";
import { PhArrowDown, PhArrowUp } from "@phosphor-icons/vue";
import { EButton } from "@eos-ui/vue/components/EButton";
import { useBoard } from "../board/board";
import { formatNumber, seatName } from "../format";
import { levelChoices } from "../sheets/levels";
import UiToolbar from "../ui/UiToolbar.vue";
import { useRematch } from "./useRematch";

/**
 * A finished game of yours against an engine, played here: how it went for you, who the engine was and where you
 * stand, and what to do next. Rematch is the one filled button (the colours swap); against an engine with levels one
 * level easier or harder plays the same clock against the neighbouring level of its ladder; Review this game reads it with the engine. Keys: R plays again.
 */
const props = defineProps<{
  /** Whether the game has been reviewed, is being, or has not been: Review this game shows until it has begun. */
  review: "none" | "asking" | "reading" | "done";
}>();
defineEmits<{ review: [] }>();

const board = useBoard();
const { overview, strength, easier, harder, starting, start } = useRematch();

const result = computed(() => {
  const s = board.summary;
  if (!s || s.status === "aborted" || !s.you) return "aborted";
  if (s.outcome === "draw") return "draw";
  return (s.outcome === "white_wins") === (s.you === "white") ? "win" : "loss";
});

const opponent = computed(() => {
  const level = strength.value;
  if (level === null) return null;
  const choice = levelChoices(overview.value).find((c) => c.value === String(level));
  return choice ?? { name: level === "full" ? "Full strength" : `Level ${level}`, text: "" };
});

/** Who it was, as the server named the seat: "Stockfish 1650", "Reckless 1400", "Stockfish 19". */
const who = computed(() => {
  const s = board.summary;
  if (!s?.you) return "the engine";
  return seatName(s.you === "white" ? s.black : s.white);
});

const headline = computed(() => {
  const them = who.value;
  return {
    win: `You beat ${them}`,
    loss: `${them} beat you`,
    draw: `You drew with ${them}`,
    aborted: "The game was aborted",
  }[result.value];
});

/** Where each side stands: the engine at this strength, and your own rating as the ladder has it. */
const context = computed(() => {
  const parts: string[] = [];
  const them = opponent.value;
  if (them?.text) parts.push(`${them.name} is rated ${them.text}`);
  const you = overview.value?.you;
  if (you) parts.push(`you are about ${formatNumber(Math.round(you.estimate.elo))}`);
  return parts.join(", ");
});

const asked = computed(() => props.review !== "none");
</script>

<template>
  <section class="end" :class="`end--${result}`" aria-label="Game over">
    <p class="headline">{{ headline }}</p>
    <p v-if="context" class="context">{{ context[0]!.toUpperCase() + context.slice(1) }}.</p>
    <UiToolbar label="Play again">
      <EButton
        variant="primary"
        :loading="starting"
        aria-keyshortcuts="R"
        title="Same opponent and clock, the colours swapped (R)"
        @click="start()"
      >
        Rematch
      </EButton>
      <EButton
        v-if="easier"
        variant="outline"
        :icon="PhArrowDown"
        :disabled="starting"
        :title="`Play ${easier.name} next, a step easier`"
        @click="start(easier.strength)"
      >
        {{ easier.name }}
      </EButton>
      <EButton
        v-if="harder"
        variant="outline"
        :icon="PhArrowUp"
        :disabled="starting"
        :title="`Play ${harder.name} next, a step harder`"
        @click="start(harder.strength)"
      >
        {{ harder.name }}
      </EButton>
      <EButton
        v-if="!asked || review === 'asking'"
        variant="outline"
        :loading="review === 'asking'"
        @click="$emit('review')"
      >
        Review this game
      </EButton>
    </UiToolbar>
  </section>
</template>

<style scoped>
.end {
  display: grid;
  gap: var(--sp-2);
  padding: var(--sp-2) var(--sp-3);
  border-radius: var(--r-1);
  background: var(--c-raise);
  box-shadow: inset var(--e-stroke-focus) 0 0 var(--_tone, var(--c-line-strong));
}

.end--win {
  --_tone: var(--c-win);
}

.end--loss {
  --_tone: var(--c-loss);
}

.end--draw {
  --_tone: var(--c-draw);
}

.headline {
  margin: 0;
  font: var(--t-head);
}

.context {
  margin: 0;
  color: var(--c-ink-2);
  font: var(--t-meta);
}
</style>
