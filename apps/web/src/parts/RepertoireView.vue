<script setup lang="ts">
import { computed, onMounted, ref, watch } from "vue";
import { RouterLink, useRouter } from "vue-router";
import { PhCaretDown, PhCaretRight } from "@phosphor-icons/vue";
import { EButton } from "@eos-ui/vue/components/EButton";
import { EIconButton } from "@eos-ui/vue/components/EIconButton";
import { ELink } from "@eos-ui/vue/components/ELink";
import { ENotice } from "@eos-ui/vue/components/ENotice";
import { ESegmented } from "@eos-ui/vue/components/ESegmented";
import { ESkeleton } from "@eos-ui/vue/components/ESkeleton";
import type { Departure, Side } from "../api/types";
import { useBoard } from "../board/board";
import { INITIAL_FEN } from "../board/position";
import { plural } from "../format";
import { departures, mainLineIds, outline, share } from "../sheets/repertoire";
import { useRepertoire } from "../stores/repertoire";
import { stored } from "../stored";

/**
 * Your openings, one colour at a time: the lines you play as a compact tree (move, games, share, score), where you and
 * your opponents leave theory, and a way to open any line in Explore. Built from your games. Hidden while the policy
 * forbids the explorer.
 */
const repertoire = useRepertoire();
const board = useBoard();
const router = useRouter();

const side = stored<Side>("repertoire.side", "white");
onMounted(() => void repertoire.load());

const rep = computed(() => repertoire.sides[side.value]);
const expanded = ref(new Set<string>());
watch(
  rep,
  (r) => {
    if (r) expanded.value = mainLineIds(r);
  },
  { immediate: true },
);
watch(side, () => {
  if (rep.value) expanded.value = mainLineIds(rep.value);
});

const rows = computed(() => (rep.value ? outline(rep.value, expanded.value) : []));
const leaves = computed(() => (rep.value ? departures(rep.value) : { yours: [], theirs: [] }));

function toggle(id: string) {
  const next = new Set(expanded.value);
  if (!next.delete(id)) next.add(id);
  expanded.value = next;
}

/** Opens a line in Explore, on the board. */
function show(fen: string, moves: string[]) {
  board.openFree(fen, moves);
  void router.push({ path: "/", query: board.address });
}

const departureLine = (d: Departure) => `${Math.ceil(d.ply / 2)}${d.ply % 2 === 1 ? "." : "…"} ${d.san}`;
</script>

<template>
  <section v-if="repertoire.available" class="sec" aria-labelledby="y-rep">
    <div class="sec-head">
      <h2 id="y-rep" class="part-title">Your openings</h2>
      <ESegmented
        v-model="side"
        label="Colour"
        hide-label
        size="sm"
        :options="[
          { value: 'white', label: 'As White' },
          { value: 'black', label: 'As Black' },
        ]"
      />
    </div>

    <ENotice v-if="repertoire.failed" tone="danger" title="Your openings did not load">{{ repertoire.failed }}</ENotice>
    <ESkeleton v-else-if="!rep" :lines="4" label="Loading your openings" />
    <p v-else-if="rep.games === 0" class="sec-note">
      No games as {{ side === "white" ? "White" : "Black" }} yet. Your lines appear after a few games from the standard
      start.
    </p>
    <template v-else>
      <p class="sec-note">
        {{ plural(rep.games, "game") }} as {{ side === "white" ? "White" : "Black" }}. Your moves are shown with a dot;
        the others are what you met.
      </p>
      <ul class="tree" aria-label="Your lines">
        <li class="head" aria-hidden="true">
          <span></span><span>Move</span><span class="r">Games</span><span class="r">Share</span
          ><span class="r">Score</span>
        </li>
        <li v-for="r in rows" :key="r.id" class="row" :style="{ '--depth': r.depth }">
          <EIconButton
            v-if="r.expandable"
            :icon="expanded.has(r.id) ? PhCaretDown : PhCaretRight"
            :label="`${expanded.has(r.id) ? 'Hide' : 'Show'} what follows ${r.number} ${r.san}`"
            size="sm"
            @click="toggle(r.id)"
          />
          <span v-else class="gap"></span>
          <EButton
            variant="quiet"
            size="sm"
            class="move"
            :class="{ 'move--you': r.yours }"
            :aria-label="`Show ${r.number} ${r.san} on the board`"
            @click="show(INITIAL_FEN, r.line)"
          >
            <span class="mono">{{ r.number }} {{ r.san }}</span>
            <span v-if="r.leaves" class="out">out of theory</span>
          </EButton>
          <span class="mono r">{{ r.games }}</span>
          <span class="mono r">{{ share(r.share) }}</span>
          <span class="mono r">{{ share(r.score) }}</span>
        </li>
      </ul>

      <div v-if="leaves.yours.length || leaves.theirs.length" class="leave">
        <div v-if="leaves.yours.length">
          <h3 class="part-title">Where you leave theory</h3>
          <ul class="plain">
            <li v-for="d in leaves.yours" :key="`${d.key}${d.uci}`" class="dep">
              <EButton variant="quiet" size="sm" @click="show(d.fen, [d.uci])">
                <span class="mono">{{ departureLine(d) }}</span>
              </EButton>
              <span class="sec-note">
                {{ plural(d.games, "game") }}, {{ share(d.score) }} score ·
                <ELink :link-component="RouterLink" :to="{ path: '/', query: { game: String(d.example) } }"
                  >latest</ELink
                >
              </span>
            </li>
          </ul>
        </div>
        <div v-if="leaves.theirs.length">
          <h3 class="part-title">Where your opponents leave</h3>
          <ul class="plain">
            <li v-for="d in leaves.theirs" :key="`${d.key}${d.uci}`" class="dep">
              <EButton variant="quiet" size="sm" @click="show(d.fen, [d.uci])">
                <span class="mono">{{ departureLine(d) }}</span>
              </EButton>
              <span class="sec-note">
                {{ plural(d.games, "game") }}, {{ share(d.score) }} score ·
                <ELink :link-component="RouterLink" :to="{ path: '/', query: { game: String(d.example) } }"
                  >latest</ELink
                >
              </span>
            </li>
          </ul>
        </div>
      </div>
    </template>
  </section>
</template>

<style scoped>
.tree {
  display: grid;
  margin: 0;
  padding: 0;
  list-style: none;
}

.head,
.row {
  display: grid;
  grid-template-columns: 2rem minmax(0, 1fr) 3.5rem 3.5rem 3.5rem;
  align-items: center;
  gap: 0 var(--e-gap-tight);
  min-width: 0;
}

.head {
  padding-block-end: var(--e-gap-tight);
  border-block-end: var(--line);
  color: var(--e-muted);
  font: var(--e-type-small);
}

.row {
  border-block-end: var(--e-stroke-hair) solid color-mix(in oklab, var(--e-border) 40%, transparent);
  font-size: 0.8125rem;
}

.row > .gap {
  inline-size: 2rem;
}

.move {
  justify-content: flex-start;
  gap: var(--e-gap-snug);
  min-width: 0;
  margin-inline-start: calc(var(--depth) * 1rem);
  text-decoration: none;
}

.move--you::before {
  inline-size: 0.375rem;
  block-size: 0.375rem;
  border-radius: 50%;
  background: var(--e-primary);
  content: "";
}

.out {
  color: var(--e-muted);
  font: var(--e-type-small);
}

.r {
  text-align: end;
}

.leave {
  display: grid;
  gap: var(--e-gap-base);
}

.dep {
  display: flex;
  flex-wrap: wrap;
  align-items: center;
  gap: 0 var(--e-gap-snug);
}
</style>
