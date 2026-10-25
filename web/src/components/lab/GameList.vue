<script setup lang="ts">
import { computed, ref, watch } from "vue";
import { RouterLink } from "vue-router";
import { EButton } from "@eos-ui/vue/components/EButton";
import { ESkeleton } from "@eos-ui/vue/components/ESkeleton";
import { useGames } from "../../api/queries";
import { ending } from "../../format";
import { outcome } from "./lab";

const props = defineProps<{ test: number }>();
const { state } = useGames(() => props.test);

const PAGE = 150;
const shown = ref(PAGE);
watch(
  () => props.test,
  () => (shown.value = PAGE),
);

/** Newest first: a running test's latest games are the ones being played. */
const games = computed(() => (state.value.data ?? []).slice().reverse());
const rows = computed(() => games.value.slice(0, shown.value));
</script>

<template>
  <ESkeleton v-if="state.status === 'pending'" :lines="6" label="Loading games" />
  <p v-else-if="!games.length" class="none">No games yet.</p>
  <template v-else>
    <ol class="games">
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
    <div v-if="games.length > shown" class="more">
      <EButton variant="quiet" size="sm" @click="shown += PAGE">
        Show {{ Math.min(PAGE, games.length - shown) }} more of {{ (games.length - shown).toLocaleString() }}
      </EButton>
    </div>
  </template>
</template>

<style scoped>
.games {
  margin: 0;
  padding: 0;
  list-style: none;
}

.game {
  display: grid;
  grid-template-columns: 3.25rem 1rem 1.25rem minmax(0, 1fr) 2.5rem;
  align-items: center;
  gap: var(--e-gap-snug);
  padding: 0.3rem var(--e-pad-control-sm);
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
}

.game-n {
  text-align: end;
}

.game-moves {
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

.none {
  margin: 0;
  padding: var(--e-pad-control-sm);
  color: var(--e-muted);
  font: var(--e-type-small);
}

.more {
  display: grid;
  padding: var(--e-gap-snug);
}
</style>
