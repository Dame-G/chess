<script setup lang="ts">
import { computed } from "vue";
import { RouterLink } from "vue-router";
import { ELink } from "@eos-ui/vue/components/ELink";
import { scoreText, seatName, speedName, timeControl } from "../format";
import { clockOf } from "../sheets/idea";
import { useBoard } from "../board/board";
import { sanLine } from "../board/position";
import { stageLabel } from "../rail/rail";
import { useGames } from "../stores/games";
import { useIdeas } from "../stores/ideas";
import { usePolicy } from "../stores/policy";
import { useVersions } from "../stores/versions";

/** Watch: a live game You are not in. The thinking engine's line and score, what the game is, and for a test game its idea. */
const board = useBoard();
const games = useGames();
const ideas = useIdeas();
const policy = usePolicy();
const versions = useVersions();

const s = computed(() => board.summary);
const t = computed(() => (policy.allowed ? games.thinking : null));
const thinker = computed(() => {
  const x = s.value;
  if (!x || !t.value) return "";
  const seat = x[t.value.side];
  return seatName(seat, versions.name(seat.version));
});
const line = computed(() => (t.value ? sanLine(board.plies[board.lastPly]?.fen ?? "", t.value.pv.slice(0, 10)) : ""));
const nps = computed(() => (t.value ? `${(t.value.nps / 1e6).toFixed(2)}M nodes/s` : ""));

const about = computed(() => {
  const x = s.value;
  if (!x) return "";
  const parts = [timeControl(x.time)];
  if (x.source.type === "lichess")
    parts.push(`${speedName[x.source.speed]} on Lichess${x.source.rated ? ", rated" : ""}`);
  if (x.opening) parts.push(x.opening.name);
  return parts.join(" · ");
});
const trial = computed(() => {
  const x = s.value;
  if (x?.source.type !== "trial") return null;
  const found = ideas.ofTrial(x.source.trial);
  if (!found) return null;
  const clock = clockOf(found.trial, ideas.trials[found.trial.id]);
  const tc = clock ? ` (${clock})` : "";
  return {
    idea: found.idea,
    text: `Game ${x.source.index + 1} of ${found.idea.title}, ${stageLabel[found.trial.stage]}${tc}`,
  };
});
</script>

<template>
  <div class="watch">
    <p class="about">{{ about }}</p>
    <p v-if="trial" class="about">
      <ELink :link-component="RouterLink" :to="`/dameless/ideas/${trial.idea.id}`">{{ trial.text }}</ELink>
    </p>
    <section v-if="policy.allowed" class="think" aria-label="Thinking">
      <template v-if="t">
        <p class="who">
          <span>{{ thinker }} is thinking</span>
          <span v-if="t.score" class="score num">{{ scoreText(t.score) }}</span>
        </p>
        <p class="pv num">{{ line }}</p>
        <p class="meta num">depth {{ t.depth }} · {{ nps }}</p>
      </template>
      <p v-else class="meta">The engine's line shows here while it thinks.</p>
    </section>
    <p v-else class="meta">{{ policy.reason }}</p>
  </div>
</template>

<style scoped>
.watch {
  display: grid;
  gap: var(--e-gap-snug);
}

.about,
.meta {
  margin: 0;
  color: var(--e-muted);
  font: var(--e-type-small);
}

.think {
  display: grid;
  gap: var(--e-gap-tight);
}

.who {
  display: flex;
  justify-content: space-between;
  margin: 0;
  font-weight: 600;
}

.score {
  font-family: var(--e-font-mono);
}

.pv {
  margin: 0;
  font: 0.8125rem/1.5 var(--e-font-mono);
}
</style>
