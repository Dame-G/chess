<script setup lang="ts">
import { computed } from "vue";
import { EListRow } from "@eos-ui/vue/components/EListRow";
import type { GameSummary, Side } from "../api/types";
import { ago, resultText, seatName, speedName, timeControl, yourResult } from "../format";
import { isLive } from "../board/modes";
import { agoShort } from "./rail";

/**
 * One game in the rail: the players, the result, accuracy once reviewed, the time control and when. A live game has a red
 * dot before its title; every row passes `live`, so finished rows keep the dot's place and all titles line up.
 */
const props = defineProps<{ game: GameSummary; nested?: boolean }>();

const name = (side: Side) => (props.game.you === side ? "You" : seatName(props.game[side]));
const live = computed(() => isLive(props.game));
const mine = computed(() => yourResult(props.game));
const where = computed(() => {
  const s = props.game.source;
  if (s.type === "lichess" || s.type === "import") return { speed: speedName[s.speed], rated: s.rated };
  return null;
});
/** Each side's accuracy to one decimal (the server sends f32s, which read as 95.80000305175781). */
const accuracy = computed(() => {
  const a = props.game.accuracy;
  return a ? { white: a.white.toFixed(1), black: a.black.toFixed(1) } : null;
});
</script>

<template>
  <EListRow :value="game.id" :live="live" live-tone="danger" trailing-stack :class="{ nested }">
    <template #title>{{ name("white") }} <span class="vs">–</span> {{ name("black") }}</template>
    <template #subtitle>
      <span v-if="accuracy" class="acc num" :title="`Accuracy: White ${accuracy.white}, Black ${accuracy.black}`"
        >{{ accuracy.white }} · {{ accuracy.black }}</span
      >
      <span class="num">{{ timeControl(game.time) }}</span>
      <span v-if="where">{{ where.speed }}</span>
      <span v-if="where?.rated">rated</span>
    </template>
    <template #trailing>
      <span class="res" :class="live ? 'res--live' : mine ? `res--${mine}` : undefined">{{
        live ? "live" : resultText(game.outcome)
      }}</span>
      <time class="when" :datetime="game.started_at" :title="ago(game.started_at)">{{
        agoShort(game.started_at)
      }}</time>
    </template>
  </EListRow>
</template>

<style scoped>
/* A trial's games sit one step in under its row. */
.nested {
  padding-inline-start: 0.75rem;
}

/* The parts of the second line (accuracy, clock, speed, rated) are told apart by a dot. */
:deep(.e-listrow-sub) > :not(:first-child)::before {
  content: "·";
  margin-inline-end: var(--e-gap-tight);
}

.vs {
  color: var(--e-muted);
  font-weight: 400;
}

.res--won {
  color: var(--e-success);
}

.res--live {
  color: var(--e-danger-ink);
}

.res--lost {
  color: var(--e-danger-ink);
}

.acc {
  color: var(--e-ink);
  font-family: var(--e-font-mono);
}
</style>
