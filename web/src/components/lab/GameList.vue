<script setup lang="ts">
import { computed, ref, watch } from "vue";
import { RouterLink } from "vue-router";
import { EButton } from "@eos-ui/vue/components/EButton";
import { EEmptyState } from "@eos-ui/vue/components/EEmptyState";
import { ESegmented } from "@eos-ui/vue/components/ESegmented";
import { ESkeleton } from "@eos-ui/vue/components/ESkeleton";
import { useGames } from "../../api/queries";
import { ending } from "../../format";
import { outcome, record } from "./lab";

/** A test's games, newest first, with how the new version did and a filter by result. */
const props = defineProps<{ test: number }>();
const { state } = useGames(() => props.test);

/** Stacked on a phone the list pushes the library far down the page, so it opens shorter there. */
const PAGE = matchMedia("(width < 75rem)").matches ? 25 : 150;
const shown = ref(PAGE);
const only = ref<"all" | "W" | "D" | "L">("all");
watch(
  () => [props.test, only.value],
  () => (shown.value = PAGE),
);
watch(
  () => props.test,
  () => (only.value = "all"),
);

/** Newest first: a running test's latest games are the ones being played. */
const games = computed(() => (state.value.data ?? []).slice().reverse());
const score = computed(() => record(games.value));
const total = computed(() => games.value.length);
const matching = computed(() =>
  only.value === "all" ? games.value : games.value.filter((g) => outcome(g) === only.value),
);
const rows = computed(() => matching.value.slice(0, shown.value));

const parts = computed(() => [
  { key: "W", n: score.value.W, name: "won" },
  { key: "D", n: score.value.D, name: "drawn" },
  { key: "L", n: score.value.L, name: "lost" },
]);
</script>

<template>
  <ESkeleton v-if="state.status === 'pending'" class="pad" :lines="6" label="Loading games" />
  <EEmptyState v-else-if="!total" bare size="sm" title="No games yet">
    They appear here as they finish, newest first.
  </EEmptyState>
  <template v-else>
    <div class="top">
      <figure class="tally">
        <div
          class="tally-bar"
          role="img"
          :aria-label="`The new version won ${score.W}, drew ${score.D} and lost ${score.L} of ${total} games`"
        >
          <span
            v-for="p in parts"
            :key="p.key"
            :class="['tally-seg', `tally-seg--${p.key}`]"
            :style="{ flexGrow: p.n }"
          ></span>
        </div>
        <figcaption class="tally-legend num">
          <span v-for="p in parts" :key="p.key" :class="`tally-${p.key}`">
            <strong>{{ p.n.toLocaleString() }}</strong> {{ p.name }}
          </span>
        </figcaption>
      </figure>
      <ESegmented
        v-model="only"
        label="Show games"
        hide-label
        size="sm"
        :options="[
          { value: 'all', label: 'All' },
          { value: 'W', label: 'Won' },
          { value: 'D', label: 'Drawn' },
          { value: 'L', label: 'Lost' },
        ]"
      />
      <div class="cols" aria-hidden="true">
        <span class="cols-n">Game</span>
        <span class="cols-side"></span>
        <span>Ending</span>
        <span class="cols-moves">Moves</span>
      </div>
    </div>
    <EEmptyState v-if="!matching.length" bare size="sm" title="No games match">
      None of this test's games ended that way.
      <template #action>
        <EButton variant="quiet" size="sm" @click="only = 'all'">Show all games</EButton>
      </template>
    </EEmptyState>
    <ol v-else class="games">
      <li v-for="g in rows" :key="g.index">
        <RouterLink class="game" :to="`/games/${test}/${g.index}`">
          <span class="game-n num">{{ g.index + 1 }}</span>
          <span
            :class="['game-side', g.white === 'dev' ? 'game-side--white' : 'game-side--black']"
            :title="`New version plays ${g.white === 'dev' ? 'White' : 'Black'}`"
          ></span>
          <span :class="['game-r', `game-r--${outcome(g)}`]">{{ outcome(g) }}</span>
          <span class="game-end">{{ ending(g.termination) }}</span>
          <span class="game-moves num">{{ Math.ceil(g.plies / 2) }}</span>
        </RouterLink>
      </li>
    </ol>
    <div v-if="matching.length > shown" class="more">
      <EButton variant="quiet" size="sm" @click="shown += PAGE">
        Show {{ Math.min(PAGE, matching.length - shown) }} more of {{ (matching.length - shown).toLocaleString() }}
      </EButton>
    </div>
  </template>
</template>

<style scoped>
.pad {
  padding: var(--e-gap-base) var(--ws-pad, var(--e-pad-card));
}

.top {
  position: sticky;
  z-index: 1;
  inset-block-start: 0;
  display: grid;
  gap: var(--e-gap-base);
  padding: var(--e-gap-base) var(--ws-pad, var(--e-pad-card)) var(--e-gap-snug);
  border-block-end: var(--e-stroke-hair) solid color-mix(in oklab, var(--e-border) 55%, transparent);
  background: var(--e-surface);
}

.tally {
  display: grid;
  gap: var(--e-gap-snug);
  margin: 0;
}

.tally-bar {
  display: flex;
  gap: 2px;
  height: 0.5rem;
  overflow: hidden;
  border-radius: var(--e-radius-sm);
  background: var(--e-surface-tint);
}

.tally-seg {
  min-width: 2px;
}

.tally-seg--W {
  background: var(--e-success);
}

.tally-seg--D {
  background: color-mix(in oklab, var(--e-border) 70%, var(--e-surface));
}

.tally-seg--L {
  background: var(--e-danger);
}

.tally-legend {
  display: flex;
  justify-content: space-between;
  color: var(--e-muted);
  font-size: var(--e-text-xs, 0.75rem);
}

.tally-legend strong {
  font-weight: 700;
}

.tally-W strong {
  color: var(--e-success);
}

.tally-L strong {
  color: var(--e-danger);
}

.cols,
.game {
  display: grid;
  grid-template-columns: 3.25rem 1rem 1.25rem minmax(0, 1fr) 2.75rem;
  align-items: center;
  gap: var(--e-gap-base);
}

.cols {
  padding-block-start: var(--e-gap-tight);
  color: var(--e-muted);
  font: var(--e-type-meta);
  letter-spacing: 0.04em;
  text-transform: uppercase;
}

.cols-n,
.cols-moves {
  text-align: end;
}

.cols-side {
  grid-column: span 2;
  white-space: nowrap;
}

.games {
  margin: 0;
  padding: 0;
  list-style: none;
}

.game {
  min-height: 2.75rem;
  padding: var(--e-gap-snug) var(--ws-pad, var(--e-pad-card));
  border-block-end: var(--e-stroke-hair) solid color-mix(in oklab, var(--e-border) 35%, transparent);
  color: inherit;
  font: var(--e-type-small);
  text-decoration: none;
}

.game:hover {
  background: var(--e-surface-tint);
}

.game:focus-visible {
  outline: var(--e-stroke-focus, 2px) solid var(--e-focus, var(--e-primary));
  outline-offset: -2px;
}

.game-n,
.game-moves {
  color: var(--e-muted);
  text-align: end;
}

.game-side {
  width: 0.625rem;
  height: 0.625rem;
  border-radius: 50%;
  box-shadow: 0 0 0 1px var(--e-border-strong);
}

.game-side--white {
  background: #fffaf3;
}

.game-side--black {
  background: #2a2420;
}

.game-r {
  font: 700 var(--e-text-sm, 0.8125rem) / 1 var(--e-font-mono);
}

.game-r--W {
  color: var(--e-success);
}

.game-r--L {
  color: var(--e-danger);
}

.game-r--D {
  color: var(--e-muted);
}

.game-end {
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
}

.more {
  display: grid;
  padding: var(--e-gap-base) var(--ws-pad, var(--e-pad-card));
}
</style>
