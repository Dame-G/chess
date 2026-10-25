<script setup lang="ts">
import { computed, onBeforeUnmount, onMounted, ref, watch } from "vue";
import { RouterLink, useRoute } from "vue-router";
import { useQuery } from "@pinia/colada";
import { ELink } from "@eos-ui/vue/components/ELink";
import { EButton } from "@eos-ui/vue/components/EButton";
import { EDescriptionList } from "@eos-ui/vue/components/EDescriptionList";
import { EInline } from "@eos-ui/vue/components/EInline";
import { ENotice } from "@eos-ui/vue/components/ENotice";
import { EPageHeader } from "@eos-ui/vue/components/EPageHeader";
import { EStack } from "@eos-ui/vue/components/EStack";
import ChessBoard from "../board/ChessBoard.vue";
import LineChart from "../charts/LineChart.vue";
import { readGame } from "../board/game";
import { play } from "../board/position";
import { api } from "../api/runner";
import { unwrap } from "../api/queries";

const route = useRoute();
const testId = computed(() => Number(route.params.id));
const index = computed(() => Number(route.params.index));

const { state } = useQuery({
  key: () => ["game", testId.value, index.value],
  query: () =>
    unwrap(
      api.GET("/api/tests/{id}/games/{index}", {
        params: { path: { id: testId.value, index: index.value } },
        parseAs: "text",
      }),
    ),
});
const game = computed(() => (state.value.data ? readGame(state.value.data) : undefined));

/** How many plies are shown: 0 is the starting position. */
const ply = ref(0);
watch(game, (g) => (ply.value = g?.plies.length ?? 0));

const position = computed(() => {
  const g = game.value;
  if (!g) return undefined;
  return play({ fen: g.startFen, moves: g.plies.slice(0, ply.value).map((p) => p.uci) });
});
const current = computed(() => (ply.value > 0 ? game.value?.plies[ply.value - 1] : undefined));

function go(to: number) {
  ply.value = Math.max(0, Math.min(game.value?.plies.length ?? 0, to));
}
function onKey(e: KeyboardEvent) {
  if (e.target instanceof HTMLInputElement || e.target instanceof HTMLTextAreaElement) return;
  if (e.key === "ArrowLeft") go(ply.value - 1);
  else if (e.key === "ArrowRight") go(ply.value + 1);
  else if (e.key === "Home") go(0);
  else if (e.key === "End") go(Infinity);
  else return;
  e.preventDefault();
}
onMounted(() => window.addEventListener("keydown", onKey));
onBeforeUnmount(() => window.removeEventListener("keydown", onKey));

/** Evaluations clamped to ±10 pawns, so one mate does not flatten the graph. */
const evals = computed(
  () => game.value?.plies.map((p) => (p.eval === null ? null : Math.max(-10, Math.min(10, p.eval)))) ?? [],
);
const title = computed(() => {
  const h = game.value?.headers;
  return h ? `${h.White} – ${h.Black}, ${h.Result}` : "Game";
});
</script>

<template>
  <EStack gap="section">
    <EPageHeader :title="title" :description="`Game ${index + 1} of test ${testId}`" />
    <ENotice v-if="state.status === 'error'" tone="danger" title="Could not load the game">
      {{ state.error?.message }}
    </ENotice>

    <div v-if="game && position" class="game">
      <EStack gap="base">
        <ChessBoard :position="position" />
        <EInline gap="snug">
          <EButton variant="outline" size="sm" icon="chevrons-left" @click="go(0)">Start</EButton>
          <EButton variant="outline" size="sm" icon="chevron-left" @click="go(ply - 1)">Back</EButton>
          <EButton variant="outline" size="sm" icon-end="chevron-right" @click="go(ply + 1)">Forward</EButton>
          <EButton variant="outline" size="sm" icon-end="chevrons-right" @click="go(Infinity)">End</EButton>
        </EInline>
      </EStack>

      <EStack gap="base">
        <EDescriptionList
          :items="[
            { label: 'Opening', text: game.headers.FEN ?? 'Start position' },
            { label: 'Ending', text: `${game.headers.Termination ?? ''}, ${game.plies.length} plies` },
            current
              ? {
                  label: `Move ${Math.ceil(ply / 2)}${ply % 2 ? '' : '…'} ${current.san}`,
                  text: [
                    current.eval !== null ? `${current.eval > 0 ? '+' : ''}${current.eval.toFixed(2)}` : null,
                    current.depth !== null ? `depth ${current.depth}` : null,
                    current.nodes !== null ? `${current.nodes.toLocaleString()} nodes` : null,
                    current.time !== null ? `${current.time}s` : null,
                  ]
                    .filter(Boolean)
                    .join(' · '),
                }
              : null,
          ]"
        />
        <LineChart
          label="Evaluation from White's side by ply"
          :x="game.plies.map((_, i) => i + 1)"
          :series="[{ label: 'eval', values: evals }]"
          :lines="[{ value: 0, label: 'even' }]"
          :height="160"
        />
        <div class="moves" role="list" aria-label="Moves">
          <EButton
            v-for="(p, i) in game.plies"
            :key="i"
            role="listitem"
            size="sm"
            :variant="i + 1 === ply ? 'secondary' : 'quiet'"
            @click="go(i + 1)"
          >
            {{ i % 2 === 0 ? `${i / 2 + 1}. ` : "" }}{{ p.san }}
          </EButton>
        </div>
        <ELink :link-component="RouterLink" :to="`/experiments`">Back to experiments</ELink>
      </EStack>
    </div>
  </EStack>
</template>

<style scoped>
.game {
  display: grid;
  grid-template-columns: minmax(0, 40rem) minmax(16rem, 1fr);
  gap: var(--e-gap-loose);
  align-items: start;
}

@media (width < 60rem) {
  .game {
    grid-template-columns: minmax(0, 1fr);
  }
}

.moves {
  display: flex;
  flex-wrap: wrap;
  gap: var(--e-space-1, 0.25rem);
  max-height: 20rem;
  overflow-y: auto;
}
</style>
