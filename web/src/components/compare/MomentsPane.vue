<script setup lang="ts">
import { computed } from "vue";
import { RouterLink } from "vue-router";
import { ELink } from "@eos-ui/vue/components/ELink";
import OpponentName from "../lichess/OpponentName.vue";
import { ago } from "../../format";
import type { Moment, PlayerStats } from "../../api/queries";
import { classes, glyphOf, moveLabel, momentPath, nameOf } from "./compare";

const props = defineProps<{ players: readonly PlayerStats[] }>();

const withMoments = computed(() =>
  props.players.filter((p) => p.notable.brilliant.length || p.notable.blunders.length || p.notable.most_accurate_game),
);
const labelOf = (c: Moment["classification"]) => classes.find((k) => k.key === c)?.label ?? c;
const win = (n: number) => `${Math.round(n)}%`;
const pawns = (n: number) => `${n} ${n === 1 ? "pawn" : "pawns"}`;
const what = (m: Moment) =>
  m.sacrifice <= 0
    ? ""
    : m.classification === "blunder"
      ? `dropped ${pawns(m.sacrifice)}`
      : `sacrificed ${pawns(m.sacrifice)}`;
</script>

<template>
  <div class="moments">
    <p v-if="!withMoments.length" class="none">
      The best moves, the worst blunders and the best and worst games of each player appear here once their games are
      reviewed.
    </p>
    <section v-for="p in withMoments" :key="p.player" class="player" :aria-label="`Moments of ${nameOf(p)}`">
      <OpponentName :name="nameOf(p)" :title="p.title" class="who" />

      <template
        v-for="part in [
          { id: 'best', title: 'Best moves', moments: p.notable.brilliant },
          { id: 'blunders', title: 'Worst blunders', moments: p.notable.blunders },
        ]"
        :key="part.id"
      >
        <h3 v-if="part.moments.length" class="part-title">{{ part.title }}</h3>
        <ul v-if="part.moments.length" class="list">
          <li v-for="m in part.moments" :key="`${m.match_id}-${m.ply}`">
            <ELink :link-component="RouterLink" :to="momentPath(m)" class="item">
              <span class="glyph" :style="{ color: `var(--q-${m.classification})` }" aria-hidden="true">
                {{ glyphOf(m.classification) }}
              </span>
              <span class="move num">{{ moveLabel(m) }}</span>
              <span class="chance num">{{ win(m.win_before) }} → {{ win(m.win_after) }}</span>
              <span class="meta">
                {{ labelOf(m.classification) }} · vs {{ m.opponent }} · {{ ago(m.played_at)
                }}<template v-if="what(m)"> · {{ what(m) }}</template>
              </span>
            </ELink>
          </li>
        </ul>
      </template>

      <template v-if="p.notable.most_accurate_game || p.notable.least_accurate_game">
        <h3 class="part-title">Games</h3>
        <ul class="list">
          <li
            v-for="g in [
              { id: 'most', label: 'Most accurate', game: p.notable.most_accurate_game },
              { id: 'least', label: 'Least accurate', game: p.notable.least_accurate_game },
            ]"
            :key="g.id"
          >
            <ELink v-if="g.game" :link-component="RouterLink" :to="`/matches/${g.game.match_id}`" class="item">
              <span class="glyph" aria-hidden="true">{{ g.id === "most" ? "▲" : "▼" }}</span>
              <span class="move">{{ g.label }}</span>
              <span class="chance num">{{ g.game.accuracy.toFixed(1) }}%</span>
              <span class="meta">vs {{ g.game.opponent }} · {{ ago(g.game.played_at) }}</span>
            </ELink>
          </li>
        </ul>
      </template>
    </section>
  </div>
</template>

<style scoped>
.moments {
  display: grid;
  align-content: start;
  gap: var(--e-gap-section);
  padding: var(--e-gap-loose) var(--ws-pad);
}

.none {
  margin: 0;
  color: var(--e-muted);
  font: var(--e-type-small);
}

.player {
  display: grid;
  gap: var(--e-gap-snug);
}

.who {
  font: var(--e-type-h5);
}

.part-title {
  margin-block-start: var(--e-gap-snug);
}

.list {
  display: grid;
  margin: 0;
  padding: 0;
  list-style: none;
}

.item {
  display: grid;
  grid-template-columns: 1.5rem auto 1fr;
  grid-template-areas:
    "glyph move chance"
    ". meta meta";
  align-items: baseline;
  column-gap: var(--e-gap-snug);
  padding-block: var(--e-gap-snug);
  border-block-end: var(--e-stroke-hair) solid color-mix(in oklab, var(--e-border) 28%, transparent);
  color: inherit;
  text-decoration: none;
}

.item:hover .move {
  color: var(--e-primary);
}

.glyph {
  grid-area: glyph;
  font-family: var(--e-font-mono);
  font-weight: 700;
  text-align: center;
}

.move {
  grid-area: move;
  font-family: var(--e-font-mono);
  font-weight: 600;
}

.chance {
  grid-area: chance;
  justify-self: end;
  color: var(--e-muted);
  font: var(--e-type-small);
}

.meta {
  grid-area: meta;
  color: var(--e-muted);
  font: var(--e-type-small);
  text-transform: none;
}
</style>
