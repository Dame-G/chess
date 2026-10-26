<script setup lang="ts">
/**
 * The board's library, the same in every mode but a test's games: start something (play, watch two engines, load a
 * game), pick up a game, and go back to a position. Live games sit on top.
 */
import { computed, ref } from "vue";
import { PhClipboardText, PhClockCounterClockwise, PhFlask, PhSquaresFour, PhSword, PhUser } from "@phosphor-icons/vue";
import { EButton } from "@eos-ui/vue/components/EButton";
import { EEmptyState } from "@eos-ui/vue/components/EEmptyState";
import { ESegmented } from "@eos-ui/vue/components/ESegmented";
import { ESkeleton } from "@eos-ui/vue/components/ESkeleton";
import MatchList from "./MatchList.vue";
import type { MatchSummary } from "./matchText";

interface RecentPosition {
  label: string;
  fen: string;
  moves: string[];
}

const props = defineProps<{
  matches: MatchSummary[];
  loading?: boolean;
  /** The match on the board. */
  current?: number;
  /** Recent positions on the free board, with a label each. */
  recent: RecentPosition[];
  /** The newest test with games, if any. */
  latest?: { id: number; name: string; games: number };
}>();
const emit = defineEmits<{
  play: [];
  engines: [];
  load: [];
  open: [id: number];
  position: [fen: string, moves: string[]];
  test: [id: number];
}>();

const kinds = computed(() => ({
  yours: props.matches.filter((m) => m.white.human || m.black.human).length,
  engines: props.matches.filter((m) => !m.lichess && !m.white.human && !m.black.human).length,
  lichess: props.matches.filter((m) => m.lichess).length,
}));
const filter = ref("all");
const options = computed(() => {
  const o = [{ value: "all", label: "All" }];
  if (kinds.value.yours) o.push({ value: "yours", label: "Yours" });
  if (kinds.value.engines) o.push({ value: "engines", label: "Engines" });
  if (kinds.value.lichess) o.push({ value: "lichess", label: "Lichess" });
  return o;
});
const shown = computed(() => {
  const f = options.value.some((o) => o.value === filter.value) ? filter.value : "all";
  // A game cancelled before the first move leaves nothing to replay.
  const list = props.matches
    .filter((m) => m.plies > 0 || m.status !== "aborted")
    .filter((m) =>
      f === "yours"
        ? m.white.human || m.black.human
        : f === "engines"
          ? !m.lichess && !m.white.human && !m.black.human
          : f === "lichess"
            ? m.lichess
            : true,
    );
  // What is being played comes first.
  return [...list.filter((m) => m.status === "running"), ...list.filter((m) => m.status !== "running")];
});
</script>

<template>
  <div class="lib">
    <div class="start">
      <EButton size="sm" :icon="PhUser" @click="emit('play')">Play</EButton>
      <EButton size="sm" variant="outline" :icon="PhSword" @click="emit('engines')">Engines</EButton>
      <EButton size="sm" variant="quiet" :icon="PhClipboardText" @click="emit('load')">Load</EButton>
    </div>

    <section aria-label="Games">
      <div class="head">
        <h3 class="section">Games</h3>
        <span v-if="matches.length" class="count num">{{ shown.length }}</span>
      </div>
      <ESegmented
        v-if="options.length > 1"
        v-model="filter"
        label="Show games"
        hide-label
        size="sm"
        :options="options"
        class="filter"
      />
      <ESkeleton v-if="loading && !matches.length" :lines="4" label="Loading games" class="pad" />
      <EEmptyState v-else-if="!shown.length" title="No games yet" icon="info" size="sm" bare class="pad">
        Play one, or have two engines play, and it is kept here to replay.
      </EEmptyState>
      <MatchList v-else :matches="shown" :current="current" @open="emit('open', $event)" />
    </section>

    <section aria-label="Positions">
      <h3 class="section">Positions</h3>
      <EButton
        variant="quiet"
        size="sm"
        block
        :icon="PhSquaresFour"
        class="row"
        @click="emit('position', 'startpos', [])"
      >
        Start position
      </EButton>
      <EButton
        v-if="latest"
        variant="quiet"
        size="sm"
        block
        :icon="PhFlask"
        class="row"
        @click="emit('test', latest.id)"
      >
        <span class="grow">{{ latest.name }}</span>
        <span class="aside num">{{ latest.games.toLocaleString() }}</span>
      </EButton>
      <EButton
        v-for="(r, i) in recent"
        :key="i"
        variant="quiet"
        size="sm"
        block
        :icon="PhClockCounterClockwise"
        class="row"
        @click="emit('position', r.fen, r.moves)"
      >
        <span class="grow num">{{ r.label }}</span>
      </EButton>
    </section>
  </div>
</template>

<style scoped>
.lib {
  display: grid;
  align-content: start;
  padding-block-end: var(--e-gap-base);
}

.start {
  display: grid;
  grid-template-columns: 1fr 1fr auto;
  gap: var(--e-gap-snug);
  padding: var(--e-gap-snug) var(--ws-pad);
  border-block-end: var(--e-stroke-hair) solid color-mix(in oklab, var(--e-border) 40%, transparent);
}

.head {
  display: flex;
  align-items: baseline;
  justify-content: space-between;
  padding-inline-end: var(--ws-pad);
}

.section {
  margin: var(--e-gap-snug) 0 0;
  padding: var(--e-gap-tight) var(--ws-pad);
  color: var(--e-muted);
  font: var(--e-type-small);
  letter-spacing: 0.04em;
  text-transform: uppercase;
}

.count {
  color: var(--e-muted);
  font: var(--e-type-small);
}

.filter {
  margin: 0 var(--ws-pad) var(--e-gap-tight);
}

.pad {
  padding: var(--e-pad-control-sm);
}

.row {
  justify-content: flex-start;
  border-radius: 0;
  text-align: start;
  text-decoration: none;
}

.grow {
  min-width: 0;
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
}

.aside {
  margin-inline-start: auto;
  color: var(--e-muted);
}
</style>
