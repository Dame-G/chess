<script setup lang="ts">
import { computed, ref, watch } from "vue";
import { useToast } from "@eos-ui/vue";
import { EActions } from "@eos-ui/vue/components/EActions";
import { EButton } from "@eos-ui/vue/components/EButton";
import { ENotice } from "@eos-ui/vue/components/ENotice";
import { ESkeleton } from "@eos-ui/vue/components/ESkeleton";
import "../board/review.css";
import type { Classification, Side } from "../api/types";
import { resultText, seatName } from "../format";
import { useBoard } from "../board/board";
import { sanLine } from "../board/position";
import { findPly, hasBetter, hint, markOf, SUMMARY_MARKS } from "../board/review";
import { useGames } from "../stores/games";
import { usePolicy } from "../stores/policy";
import { useVersions } from "../stores/versions";
import { useUi } from "../shell/ui";

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

const s = computed(() => board.summary);
const review = computed(() => games.review);
const sides: Side[] = ["white", "black"];
const name = (side: Side) => {
  const x = s.value;
  if (!x) return side;
  return x.you === side ? "You" : seatName(x[side], versions.name(x[side].version));
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

const fmt = (n: number | null) => (n === null ? "–" : n.toFixed(1));
</script>

<template>
  <div class="review">
    <p class="result">
      <span class="num">{{ resultText(s?.outcome ?? null) }}</span>
      <span v-if="s?.ending" class="ending">{{ s.ending.replace(/_/g, " ") }}</span>
      <span v-if="s?.opening" class="ending">· {{ s.opening.name }}</span>
    </p>

    <p v-if="!policy.allowed" class="muted">{{ policy.reason }}</p>
    <ESkeleton v-else-if="games.reviewLoading && !review" :lines="4" label="Loading the review" />
    <template v-else-if="review">
      <div class="sides">
        <section v-for="side in sides" :key="side" class="side" :aria-label="`${name(side)}'s review`">
          <p class="side-head">
            <span class="side-name">{{ name(side) }}</span>
            <span class="acc num" title="Accuracy">{{ fmt(review.summary[side].accuracy) }}</span>
          </p>
          <p class="acpl num">{{ review.summary[side].acpl ?? "–" }} average centipawn loss</p>
          <div class="counts">
            <template v-for="c in SUMMARY_MARKS" :key="c">
              <EButton
                v-if="review.summary[side].counts[c]"
                size="sm"
                variant="quiet"
                class="count"
                :class="`rv-${c}`"
                :aria-label="`${review.summary[side].counts[c]} ${markOf(c).label}: go to the next`"
                @click="jump(side, c)"
              >
                <span class="count-n num">{{ review.summary[side].counts[c] }}</span>
                <span class="count-l">{{ markOf(c).label }}</span>
              </EButton>
              <span v-else class="count count--none">
                <span class="count-n num">0</span>
                <span class="count-l">{{ markOf(c).label }}</span>
              </span>
            </template>
          </div>
        </section>
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
  gap: var(--e-gap-base);
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

.sides {
  display: grid;
  grid-template-columns: 1fr 1fr;
  gap: var(--e-gap-base);
}

.side {
  display: grid;
  gap: var(--e-gap-tight);
  align-content: start;
  min-width: 0;
}

.side-head {
  display: flex;
  justify-content: space-between;
  align-items: baseline;
  margin: 0;
}

.side-name {
  overflow: hidden;
  font-weight: 600;
  text-overflow: ellipsis;
  white-space: nowrap;
}

.acc {
  font: 700 1.375rem/1 var(--e-font-mono);
}

.acpl {
  margin: 0;
  color: var(--e-muted);
  font: var(--e-type-small);
}

.counts {
  display: grid;
}

.count {
  justify-content: flex-start;
  gap: var(--e-gap-snug);
  min-height: 1.75rem;
  padding-inline: 0.25rem;
  text-decoration: none;
}

.count--none {
  display: flex;
  align-items: center;
  color: var(--e-muted);
}

.count--none .count-n {
  color: var(--e-muted);
}

.count-n {
  min-inline-size: 2ch;
  color: var(--rv);
  font: 700 0.875rem/1 var(--e-font-mono);
  text-align: end;
}

.count-l {
  font: var(--e-type-small);
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
