<script setup lang="ts">
import { computed } from "vue";
import { EListRow } from "@eos-ui/vue/components/EListRow";
import type { GameSummary, Side } from "../api/types";
import { formatDateTime, resultOf, seatName, speedName, timeControl } from "../format";
import { isLive } from "../board/modes";
import UiLive from "../ui/UiLive.vue";
import type { PlayerRef } from "../ui/player";
import UiPlayer from "../ui/UiPlayer.vue";
import UiResult from "../ui/UiResult.vue";
import { agoShort } from "./rail";

/**
 * One game in the rail, in two lines. The first names the players (white, then black; you are "You") and ends in
 * the result from your side, or a dot while the game is on; the second is the clock, the speed and whether it was
 * rated, with each side's accuracy once the game has been reviewed, and when it started at its end.
 */
const props = defineProps<{ game: GameSummary }>();

const who = (side: Side): PlayerRef => {
  const seat = props.game[side];
  return { name: props.game.you === side ? "You" : seatName(seat), key: seat.player };
};
const live = computed(() => isLive(props.game));

/** The clock, then speed and rated where Lichess says them. */
const meta = computed(() => {
  const s = props.game.source;
  const parts = [timeControl(props.game.time)];
  if (s.type === "lichess" || s.type === "import") {
    parts.push(speedName[s.speed]);
    if (s.rated) parts.push("rated");
  }
  return parts.join(" · ");
});

/** Each side's accuracy, whole numbers, once reviewed. */
const accuracy = computed(() => {
  const a = props.game.accuracy;
  return a
    ? {
        text: `${Math.round(a.white)} / ${Math.round(a.black)}`,
        title: `Accuracy: White ${a.white.toFixed(1)}, Black ${a.black.toFixed(1)}`,
      }
    : null;
});
</script>

<template>
  <EListRow :value="game.id" trailing-stack class="game-row">
    <template #title>
      <UiPlayer :seat="who('white')" class="side" />
      <span class="vs">vs</span>
      <UiPlayer :seat="who('black')" class="side" />
    </template>
    <template #subtitle>
      <span class="meta truncate">{{ meta }}</span>
      <span v-if="accuracy" class="acc num" :title="accuracy.title">{{ accuracy.text }}</span>
    </template>
    <template #trailing>
      <UiLive v-if="live" label="Live" :pulse="game.plies" class="dot" />
      <UiResult v-else :result="resultOf(game.outcome)" :you="game.you" />
      <time class="when" :datetime="game.started_at" :title="formatDateTime(game.started_at)">{{
        agoShort(game.started_at)
      }}</time>
    </template>
  </EListRow>
</template>

<style scoped>
.side {
  flex: 0 1 auto;
}

.vs {
  flex: none;
  color: var(--c-ink-3);
  font: var(--t-meta);
}

.meta {
  min-width: 0;
}

.acc {
  flex: none;
  margin-inline-start: auto;
  color: var(--c-ink-2);
  font: var(--t-meta);
}

.dot {
  margin-block: var(--sp-1);
}

.when {
  color: var(--c-ink-2);
  font: var(--t-meta);
}
</style>
