<script setup lang="ts">
/**
 * The opening explorer: what has been played from a position, in Lichess's games, the masters' or your own.
 *
 * Props
 * - `fen`: the position on the board.
 * - `orientation` ("white" | "black", default "white"): the board's side. Your own games are those you played with it.
 * - `disabled`: shows nothing and asks nothing, for a mode where the explorer does not belong.
 * - `wait`: the debounce in ms (default 200) while stepping through moves.
 *
 * Emits
 * - `play(uci)`: a move row was clicked, in standard UCI (castling as `e1g1`).
 *
 * States: loading (the previous rows stay, dimmed), no games, not logged in (links to /lichess), locked by fair
 * play, rate-limited and failed. It needs a router for the link and the Pinia Colada plugin for the account.
 */
import { computed, ref } from "vue";
import { useQuery } from "@pinia/colada";
import { EEmptyState } from "@eos-ui/vue/components/EEmptyState";
import { ELink } from "@eos-ui/vue/components/ELink";
import { ESegmented } from "@eos-ui/vue/components/ESegmented";
import { ESelect } from "@eos-ui/vue/components/ESelect";
import { ESkeleton } from "@eos-ui/vue/components/ESkeleton";
import { EButton } from "@eos-ui/vue/components/EButton";
import { api } from "../api/runner";
import { unwrap } from "../api/queries";
import ExplorerNotice from "./ExplorerNotice.vue";
import { compact, shares, standardUci, useExplorer, type Database, type ExplorerGame } from "./useExplorer";

const props = withDefaults(
  defineProps<{ fen: string; orientation?: "white" | "black"; disabled?: boolean; wait?: number }>(),
  {
    orientation: "white",
    wait: 200,
  },
);
const emit = defineEmits<{ play: [uci: string] }>();

const me = useQuery({ key: ["lichess", "me"], query: () => unwrap(api.GET("/api/lichess/me")), staleTime: 60_000 });
const username = computed(() => me.data.value?.account?.username);

const db = ref<Database>("lichess");
const dbs = computed(() => [
  { value: "lichess" as const, label: "Lichess" },
  { value: "masters" as const, label: "Masters" },
  { value: "player" as const, label: "Me", disabled: !username.value },
]);
const speeds = ref(["blitz", "rapid", "classical"]);
const ratings = ref(["1600", "1800", "2000", "2200", "2500"]);
const speedOptions = ["ultraBullet", "bullet", "blitz", "rapid", "classical", "correspondence"].map((value) => ({
  value,
  label: value === "ultraBullet" ? "Ultrabullet" : value[0]!.toUpperCase() + value.slice(1),
}));
const ratingOptions = ["400", "1000", "1200", "1400", "1600", "1800", "2000", "2200", "2500"].map((value) => ({
  value,
  label: value === "2500" ? "2500+" : value,
}));

const filters = computed(() => ({
  db: db.value,
  speeds: speeds.value,
  ratings: ratings.value,
  player: username.value,
  color: props.orientation,
}));
const { answer, state, message } = useExplorer(() => (props.disabled ? "" : props.fen), filters, props.wait);
// A position of "" asks something silly, so a disabled explorer asks nothing at all.
const failed = computed(() => (["login", "locked", "limited", "error"] as const).find((s) => s === state.value));
const busy = computed(() => state.value === "loading");

const total = computed(() => (answer.value ? answer.value.white + answer.value.draws + answer.value.black : 0));
const rows = computed(() =>
  (answer.value?.moves ?? []).map((m) => ({
    ...m,
    uciStandard: standardUci(props.fen, m.uci),
    bar: shares(m.white, m.draws, m.black),
    rating: m.averageRating ?? m.averageOpponentRating ?? undefined,
  })),
);
const sum = computed(() =>
  answer.value ? shares(answer.value.white, answer.value.draws, answer.value.black) : undefined,
);
const games = computed(() => {
  const a = answer.value;
  if (!a) return [];
  const seen = new Set<string>();
  return [...(a.topGames ?? []), ...(a.recentGames ?? [])]
    .filter((g) => !seen.has(g.id) && seen.add(g.id))
    .map((g) => ({
      id: g.id,
      href: `https://lichess.org/${g.id}`,
      names: `${label(g.white)} – ${label(g.black)}`,
      result: result(g),
      year: year(g),
    }));
});

const label = (p: ExplorerGame["white"]) => (p.rating ? `${p.name} ${p.rating}` : p.name);
const result = (g: ExplorerGame) => (g.winner === "white" ? "1-0" : g.winner === "black" ? "0-1" : "½-½");
const year = (g: ExplorerGame) => g.year ?? g.month?.slice(0, 4) ?? "";
const pct = (n: number) => (n >= 12 ? `${n}%` : "");
</script>

<template>
  <section v-if="!disabled" class="explorer" aria-label="Opening explorer" :aria-busy="busy">
    <div class="bar">
      <ESegmented v-model="db" label="Games from" hide-label size="sm" :options="dbs" />
      <div v-if="db !== 'masters'" class="filters">
        <ESelect
          v-model="speeds"
          label="Speeds"
          hide-label
          size="sm"
          multiple
          placeholder="All speeds"
          :options="speedOptions"
        />
        <ESelect
          v-if="db === 'lichess'"
          v-model="ratings"
          label="Ratings"
          hide-label
          size="sm"
          multiple
          placeholder="All ratings"
          :options="ratingOptions"
        />
      </div>
    </div>

    <ExplorerNotice
      v-if="failed && !(answer && failed === 'limited')"
      :state="failed"
      :message="message"
      what="explorer"
    />
    <p v-else-if="failed" class="quiet">Lichess asked us to slow down; showing the last answer.</p>

    <ESkeleton v-if="!answer && !failed" :lines="5" label="Loading the explorer" class="pad" />
    <template v-else-if="answer">
      <p v-if="answer.opening" class="opening">
        <span class="eco num">{{ answer.opening.eco }}</span> {{ answer.opening.name }}
      </p>
      <EEmptyState v-if="!total" title="No games" icon="info" size="sm" bare class="pad">
        Nobody in this database has reached this position.
      </EEmptyState>
      <div v-else class="table" :class="{ dim: busy }">
        <ul class="moves" aria-label="Moves played">
          <li v-for="m in rows" :key="m.uci" class="move">
            <EButton size="sm" variant="quiet" class="row" @click="emit('play', m.uciStandard)">
              <span class="san">{{ m.san }}</span>
              <span class="count num">{{ compact(m.white + m.draws + m.black) }}</span>
              <span class="rating num" :title="m.rating ? 'Average rating' : undefined">{{ m.rating ?? "" }}</span>
              <span
                class="result"
                role="img"
                :aria-label="`White ${m.bar.white}%, draws ${m.bar.draws}%, black ${m.bar.black}%`"
              >
                <span class="w num" :style="{ flexBasis: `${m.bar.white}%` }">{{ pct(m.bar.white) }}</span>
                <span class="d num" :style="{ flexBasis: `${m.bar.draws}%` }">{{ pct(m.bar.draws) }}</span>
                <span class="b num" :style="{ flexBasis: `${m.bar.black}%` }">{{ pct(m.bar.black) }}</span>
              </span>
            </EButton>
          </li>
        </ul>
        <div v-if="sum" class="row total">
          <span class="san">Total</span>
          <span class="count num">{{ compact(total) }}</span>
          <span class="rating" />
          <span class="result" role="img" :aria-label="`White ${sum.white}%, draws ${sum.draws}%, black ${sum.black}%`">
            <span class="w num" :style="{ flexBasis: `${sum.white}%` }">{{ pct(sum.white) }}</span>
            <span class="d num" :style="{ flexBasis: `${sum.draws}%` }">{{ pct(sum.draws) }}</span>
            <span class="b num" :style="{ flexBasis: `${sum.black}%` }">{{ pct(sum.black) }}</span>
          </span>
        </div>
        <ul v-if="games.length" class="games" aria-label="Games">
          <li v-for="g in games" :key="g.id" class="game">
            <span class="num res">{{ g.result }}</span>
            <ELink :href="g.href" external class="names">{{ g.names }}</ELink>
            <span class="num year">{{ g.year }}</span>
          </li>
        </ul>
      </div>
    </template>
  </section>
</template>

<style scoped>
.explorer {
  display: grid;
  align-content: start;
  min-width: 0;
}

.bar {
  display: grid;
  gap: var(--e-gap-snug);
  padding: var(--e-gap-snug) var(--ws-pad);
}

.filters {
  display: grid;
  grid-template-columns: repeat(auto-fit, minmax(8rem, 1fr));
  gap: var(--e-gap-snug);
}

.pad {
  padding: var(--e-gap-base) var(--ws-pad);
}

.quiet {
  margin: 0;
  padding: var(--e-gap-snug) var(--ws-pad);
  color: var(--e-muted);
  font: var(--e-type-small);
}

.opening {
  margin: 0;
  padding: var(--e-gap-snug) var(--ws-pad);
  border-block-start: var(--e-stroke-hair) solid color-mix(in oklab, var(--e-border) 40%, transparent);
  font: var(--e-type-small);
}

.eco {
  color: var(--e-muted);
  font-family: var(--e-font-mono);
}

.table {
  display: grid;
  transition: opacity 0.12s;
}

.dim {
  opacity: 0.5;
}

.moves,
.games {
  display: grid;
  margin: 0;
  padding: 0;
  list-style: none;
}

.row {
  display: grid;
  width: 100%;
  min-height: 1.75rem;
  grid-template-columns: 3.25rem 3rem 2.75rem minmax(0, 1fr);
  gap: var(--e-gap-snug);
  padding-block: 0;
  padding-inline: var(--ws-pad);
  font-weight: 400;
  place-items: center start;
  text-align: start;
}

.total {
  display: grid;
  min-height: 1.75rem;
  grid-template-columns: 3.25rem 3rem 2.75rem minmax(0, 1fr);
  align-items: center;
  gap: var(--e-gap-snug);
  padding-inline: var(--ws-pad);
  border-block-start: var(--e-stroke-hair) solid color-mix(in oklab, var(--e-border) 40%, transparent);
  color: var(--e-muted);
}

.san {
  font-family: var(--e-font-mono);
  font-weight: 600;
}

.count,
.rating {
  color: var(--e-muted);
  font: var(--e-type-small);
  font-family: var(--e-font-mono);
}

.result {
  display: flex;
  width: 100%;
  height: 1.25rem;
  overflow: hidden;
  border-radius: var(--e-radius-sm);
  box-shadow: inset 0 0 0 1px rgb(0 0 0 / 14%);
  font: 600 0.6875rem / 1.25rem var(--e-font-mono);
  text-align: center;
}

.result > span {
  min-width: 0;
  overflow: hidden;
}

.w {
  background: #f4ede2;
  color: #2a2420;
}

.d {
  background: #8d8780;
  color: #fff;
}

.b {
  background: #2a2420;
  color: #f4ede2;
}

.games {
  margin-block-start: var(--e-gap-snug);
  border-block-start: var(--e-stroke-hair) solid color-mix(in oklab, var(--e-border) 40%, transparent);
}

.game {
  display: grid;
  grid-template-columns: 2.5rem minmax(0, 1fr) auto;
  align-items: center;
  gap: var(--e-gap-snug);
  padding: 0.1875rem var(--ws-pad);
  font: var(--e-type-small);
}

.res,
.year {
  color: var(--e-muted);
  font-family: var(--e-font-mono);
}

.names {
  min-width: 0;
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
}

@media (prefers-reduced-motion: reduce) {
  .table {
    transition: none;
  }
}
</style>
