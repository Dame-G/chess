<script setup lang="ts">
import { computed, ref, watch } from "vue";
import { useToast } from "@eos-ui/vue";
import { EActions } from "@eos-ui/vue/components/EActions";
import { EButton } from "@eos-ui/vue/components/EButton";
import { ENotice } from "@eos-ui/vue/components/ENotice";
import { ESkeleton } from "@eos-ui/vue/components/ESkeleton";
import "../board/review.css";
import type { Classification, Side } from "../api/types";
import { endingText, resultText, seatName } from "../format";
import { useBoard } from "../board/board";
import { sanLine } from "../board/position";
import { findPly, hasBetter, hint, markOf, SUMMARY_MARKS } from "../board/review";
import { rematchRequest } from "../board/rematch";
import { useDameless } from "../stores/dameless";
import { useGames } from "../stores/games";
import { usePolicy } from "../stores/policy";
import { useVersions } from "../stores/versions";
import { useUi } from "../shell/ui";
import { useRouter } from "vue-router";

/**
 * Review: a finished game read by the engine. Accuracy per side, the marks counted (each jumps to the next such
 * move), the move on the board judged, the better move and its line, and Play this out. Not reviewed yet: one button.
 */
const board = useBoard();
const games = useGames();
const policy = usePolicy();
const versions = useVersions();
const ui = useUi();
const toast = useToast();
const dameless = useDameless();
const router = useRouter();

const s = computed(() => board.summary);
const review = computed(() => games.review);
const sides: Side[] = ["white", "black"];
const name = (side: Side) => {
  const x = s.value;
  if (!x) return side;
  return x.you === side ? "You" : seatName(x[side]);
};

const move = computed(() => (review.value && board.ply > 0 ? review.value.plies[board.ply - 1] : undefined));
const showLine = ref(false);
watch(move, () => (showLine.value = false));
const betterLine = computed(() => {
  const m = move.value;
  if (!m) return "";
  const before = board.plies[m.ply - 1]?.fen ?? "";
  return sanLine(before, m.best_pv);
});

function jump(side: Side, c: Classification) {
  const r = review.value;
  if (!r) return;
  const ply = findPly(r.plies, board.ply, (p) => p.side === side && p.classification === c);
  if (ply !== undefined) board.go(ply);
}

/** A local game against Dameless from the position before the move on the board. */
function playThisOut() {
  const g = board.game;
  if (!g) return;
  const upto = Math.max(0, board.ply - 1);
  ui.startNewGame({ fen: board.plies[0]?.fen ?? "", moves: g.moves.slice(0, upto).map((m) => m.uci) });
}

/** A rematch: the same opponent and clock with the colours swapped, from a new book line if the game began from one. */
const starting = ref(false);
const canRematch = computed(() => {
  const g = board.game;
  return !!g && rematchRequest(g, { champion: versions.current?.id, levels: dameless.levels }) !== null;
});
async function rematch() {
  const g = board.game;
  if (!g) return;
  starting.value = true;
  try {
    if (!dameless.levels) await dameless.loadLevels().catch(() => {});
    const request = rematchRequest(g, {
      champion: versions.current?.id,
      levels: dameless.levels,
      started: games.starts[String(g.summary.id)] ?? null,
    });
    if (!request) return;
    const made = await games.start(request);
    await router.push({ path: "/", query: { game: String(made.id) } });
  } catch (e) {
    toast.show({
      title: "The rematch did not start",
      message: e instanceof Error ? e.message : String(e),
      tone: "warning",
    });
  } finally {
    starting.value = false;
  }
}

const asking = ref(false);
const asked = ref(false);
async function requestReview() {
  const id = board.gameId;
  if (id === null) return;
  asking.value = true;
  try {
    await games.requestReview([id]);
    asked.value = true;
  } catch (e) {
    toast.show({
      title: "The review did not start",
      message: e instanceof Error ? e.message : String(e),
      tone: "warning",
    });
  } finally {
    asking.value = false;
  }
}
watch(
  () => board.gameId,
  () => (asked.value = false),
);

const fmt = (n: number | null | undefined) => (n == null ? "–" : n.toFixed(1));
</script>

<template>
  <div class="review">
    <p class="result">
      <span class="num">{{ resultText(s?.outcome ?? null) }}</span>
      <span v-if="s?.ending" class="ending">{{ endingText(s.ending, s.outcome) }}</span>
      <span v-if="s?.opening" class="ending">· {{ s.opening.name }}</span>
    </p>

    <EActions v-if="canRematch">
      <EButton size="sm" variant="outline" :loading="starting" @click="rematch">Rematch</EButton>
    </EActions>

    <p v-if="!policy.allowed" class="muted">{{ policy.reason }}</p>
    <ESkeleton v-else-if="games.reviewLoading && !review" :lines="9" label="Loading the review" />
    <template v-else-if="review">
      <div class="tally" role="table" aria-label="Marks by side">
        <div class="trow thead" role="row">
          <span role="presentation"></span>
          <span v-for="side in sides" :key="side" role="columnheader" class="who">
            <span class="who-name">{{ name(side) }}</span>
            <span class="acc num" title="Accuracy">{{ fmt(review.summary[side].accuracy) }}</span>
          </span>
        </div>
        <div class="trow acpl" role="row">
          <span role="rowheader">Average centipawn loss</span>
          <span v-for="side in sides" :key="side" role="cell" class="num">{{ review.summary[side].acpl ?? "–" }}</span>
        </div>
        <div v-for="c in SUMMARY_MARKS" :key="c" class="trow" :class="`rv-${c}`" role="row">
          <span role="rowheader" class="label">{{ markOf(c).label }}</span>
          <span v-for="side in sides" :key="side" role="cell">
            <EButton
              v-if="review.summary[side].counts[c]"
              size="sm"
              variant="quiet"
              class="count"
              :aria-label="`${name(side)}: ${review.summary[side].counts[c]} ${markOf(c).label}, go to the next`"
              @click="jump(side, c)"
            >
              {{ review.summary[side].counts[c] }}
            </EButton>
            <span v-else class="count count--none num">0</span>
          </span>
        </div>
      </div>

      <section v-if="move" class="move" aria-label="This move">
        <p class="hint" :class="markOf(move.classification).tone ? `rv-${move.classification}` : undefined">
          <span class="ply num"
            >{{ Math.ceil(move.ply / 2) }}{{ move.side === "white" ? "." : "…" }} {{ move.san }}</span
          >
          {{ hint(move) }}
        </p>
        <p v-if="showLine" class="line num">{{ betterLine }}</p>
        <EActions>
          <EButton
            v-if="hasBetter(move)"
            size="sm"
            variant="outline"
            :aria-pressed="showLine"
            @click="showLine = !showLine"
          >
            Show line
          </EButton>
          <EButton size="sm" variant="outline" @click="playThisOut">Play this out</EButton>
        </EActions>
      </section>
      <p v-else class="muted">Step through the moves to see each one judged. X goes to the next mistake.</p>
    </template>
    <template v-else>
      <ENotice v-if="asked" tone="info" title="Reviewing this game">It takes about 20 seconds.</ENotice>
      <template v-else>
        <p class="muted">Not reviewed yet. Dameless reads every move and judges it, in about 20 seconds.</p>
        <EActions>
          <EButton variant="secondary" :loading="asking" @click="requestReview">Review this game</EButton>
        </EActions>
      </template>
    </template>
  </div>
</template>

<style scoped>
.review {
  display: grid;
  gap: var(--e-gap-snug);
}

.result {
  display: flex;
  flex-wrap: wrap;
  gap: var(--e-gap-tight);
  align-items: baseline;
  margin: 0;
  font-weight: 700;
}

.ending {
  color: var(--e-muted);
  font: var(--e-type-small);
}

.ending::first-letter {
  text-transform: uppercase;
}

.muted {
  margin: 0;
  color: var(--e-muted);
  font: var(--e-type-small);
}

/* One table for both sides: the marks read across, You against Dameless, and the panel stays short. */
.tally {
  display: grid;
  font: var(--e-type-small);
}

.trow {
  display: grid;
  grid-template-columns: minmax(0, 1fr) 5.5rem 5.5rem;
  align-items: center;
  text-align: end;
}

.trow > :first-child {
  text-align: start;
}

.thead {
  align-items: end;
  padding-block-end: var(--e-gap-tight);
}

.who-name {
  display: block;
  overflow: hidden;
  font-weight: 600;
  font-size: var(--e-type-body-size, 0.9375rem);
  text-overflow: ellipsis;
  white-space: nowrap;
}

.acc {
  display: block;
  font: 700 1.375rem/1.2 var(--e-font-mono);
}

.acpl {
  padding-block-end: var(--e-gap-snug);
  border-block-end: var(--line);
  color: var(--e-muted);
}

.acpl .num {
  font-family: var(--e-font-mono);
}

.label {
  color: var(--rv, var(--e-ink));
}

.count {
  height: 1.625rem;
  padding-inline: 0.5rem;
  color: var(--rv);
  font: 700 0.8125rem/1 var(--e-font-mono);
  text-decoration: none;
}

.count--none {
  display: inline-block;
  padding: 0 0.5rem;
  color: var(--e-muted);
  font: 700 0.8125rem/1.625rem var(--e-font-mono);
  opacity: 0.7;
}

.move {
  display: grid;
  gap: var(--e-gap-snug);
  padding-block-start: var(--e-gap-snug);
  border-block-start: var(--line);
}

.hint {
  margin: 0;
}

.hint .ply {
  margin-inline-end: var(--e-gap-tight);
  color: var(--rv, var(--e-ink));
  font-weight: 700;
}

.line {
  margin: 0;
  font: 0.8125rem/1.5 var(--e-font-mono);
}
</style>
