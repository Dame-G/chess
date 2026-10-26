<script setup lang="ts">
import { PhChartLine } from "@phosphor-icons/vue";
import { EButton } from "@eos-ui/vue/components/EButton";
import { ENotice } from "@eos-ui/vue/components/ENotice";
import { ESkeleton } from "@eos-ui/vue/components/ESkeleton";
import { ESpinner } from "@eos-ui/vue/components/ESpinner";
import { computed } from "vue";

/** Where a game's review stands when there is none to show yet: the way to ask for it, or how it is coming along. */
const props = defineProps<{
  loading: boolean;
  /** A review job covers this game and has not finished. */
  busy: boolean;
  /** It waits for a game of yours on Lichess to end. */
  paused: boolean;
  /** How far the job's games have got, when it has reported. */
  progress?: { done: number; total: number };
  /** A game of yours is on at Lichess: analysis, and so a review, is off. */
  locked: boolean;
  error: string;
}>();
const emit = defineEmits<{ start: [] }>();

const sentence = computed(() => {
  if (props.paused) return "Paused while you play on Lichess";
  const p = props.progress;
  if (p && p.total > 1) return `Reviewing, ${p.done} of ${p.total} games done`;
  return props.progress ? "Reviewing this game" : "In the queue for review";
});
</script>

<template>
  <section class="rs" aria-label="Review">
    <ESkeleton v-if="loading" :height="20" width="60%" />
    <p v-else-if="busy" class="say" role="status"><ESpinner size="sm" silent /> {{ sentence }}</p>
    <template v-else>
      <p class="say">
        <template v-if="locked">Reviews start again when your game on Lichess ends.</template>
        <template v-else>Not reviewed yet. A review judges every move and scores each side’s accuracy.</template>
      </p>
      <div class="act">
        <EButton variant="outline" size="sm" :icon="PhChartLine" :disabled="locked" @click="emit('start')">
          Review this game
        </EButton>
      </div>
    </template>
    <ENotice v-if="error" tone="danger" title="The review did not start" :role="false">{{ error }}</ENotice>
  </section>
</template>

<style scoped>
.rs {
  display: grid;
  gap: var(--e-gap-snug);
  padding: var(--e-gap-base) var(--ws-pad);
}

.say {
  display: flex;
  align-items: center;
  gap: var(--e-gap-snug);
  margin: 0;
  color: var(--e-muted);
  font: var(--e-type-small);
}

.say :deep(svg) {
  flex: none;
}

.act {
  display: flex;
  gap: var(--e-gap-snug);
}
</style>
