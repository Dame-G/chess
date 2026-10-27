<script setup lang="ts">
import { computed } from "vue";
import { EListRow } from "@eos-ui/vue/components/EListRow";
import type { GameSummary, Side } from "../api/types";
import { ago, resultText, seatName, speedName, timeControl, yourResult } from "../format";
import { isLive } from "../board/modes";

/** One game in the rail: the players, the result, accuracy once reviewed, the time control and when. */
const props = defineProps<{ game: GameSummary; nested?: boolean }>();

const name = (side: Side) => (props.game.you === side ? "You" : seatName(props.game[side]));
const live = computed(() => isLive(props.game));
const mine = computed(() => yourResult(props.game));
const where = computed(() => {
  const s = props.game.source;
  if (s.type === "lichess" || s.type === "import") return `${speedName[s.speed]}${s.rated ? " · rated" : ""}`;
  return null;
});
/** Each side's accuracy to one decimal (the server sends f32s, which read as 95.80000305175781). */
const accuracy = computed(() => {
  const a = props.game.accuracy;
  return a ? { white: a.white.toFixed(1), black: a.black.toFixed(1) } : null;
});
</script>

<template>
  <EListRow :value="game.id" :live="live">
    <template v-if="nested" #leading><span class="indent" aria-hidden="true"></span></template>
    <template #title> {{ name("white") }} <span class="vs">–</span> {{ name("black") }} </template>
    <template #subtitle>
      <span v-if="accuracy" class="acc num" :title="`Accuracy: White ${accuracy.white}, Black ${accuracy.black}`"
        >{{ accuracy.white }} · {{ accuracy.black }}</span
      >
      <span class="num">{{ timeControl(game.time) }}</span>
      <span v-if="where">{{ where }}</span>
    </template>
    <template #trailing>
      <span class="end">
        <span class="res" :class="mine ? `res--${mine}` : undefined">{{
          live ? "live" : resultText(game.outcome)
        }}</span>
        <span class="when">{{ ago(game.started_at) }}</span>
      </span>
    </template>
  </EListRow>
</template>

<style scoped>
.indent {
  inline-size: 0.75rem;
}

.vs {
  color: var(--e-muted);
  font-weight: 400;
}

.res--won {
  color: var(--e-success);
}

.res--lost {
  color: var(--e-danger);
}

.acc {
  color: var(--e-ink);
  font-family: var(--e-font-mono);
}

.end {
  display: grid;
  justify-items: end;
}

.when {
  color: var(--e-muted);
  font: var(--e-type-small);
  font-weight: 400;
}
</style>
