<script setup lang="ts">
import { computed, onMounted, ref } from "vue";
import { EActions } from "@eos-ui/vue/components/EActions";
import { EButton } from "@eos-ui/vue/components/EButton";
import { ESwitch } from "@eos-ui/vue/components/ESwitch";
import { ETextarea } from "@eos-ui/vue/components/ETextarea";
import { buildingText, scoreText } from "../format";
import { useBoard } from "../board/board";
import { inspectPosition, sanLine, toPgn } from "../board/position";
import { useEvaluation } from "../stores/evaluation";
import { usePolicy } from "../stores/policy";
import { useRepertoire } from "../stores/repertoire";
import { useWork } from "../stores/work";
import CopyRow from "./CopyRow.vue";
import { repertoireNote } from "../sheets/repertoire";
import { useUi } from "../shell/ui";

/**
 * Explore: free analysis from any position. Moves are played freely into a tree of variations; the engine's lines
 * follow the board (Space turns it on and off); a FEN or PGN pasted opens on the board; Play from here starts a game.
 * The opening explorer and the tablebase come here next.
 */
const board = useBoard();
const evaluation = useEvaluation();
const policy = usePolicy();
const ui = useUi();
const work = useWork();
const repertoire = useRepertoire();

/** The first engine build can take a minute: say so, rather than "Thinking…" for as long. */
const waiting = computed(() => {
  if (evaluation.error) return `The engine did not start: ${evaluation.error}`;
  return work.building ? buildingText(work.building.title) : "Thinking…";
});

onMounted(() => {
  if (repertoire.available && !repertoire.sides.white) void repertoire.load();
});
const yours = computed(() => (repertoire.available ? repertoireNote(repertoire.sides, fen.value) : null));

const fen = computed(() => board.node.fen);
const line = computed(() => board.tree.line(board.node));
const pgn = () => toPgn(line.value);

/** The engine's lines, only while they are of the position on the board. */
const lines = computed(() => {
  const info = evaluation.info;
  const t = evaluation.target;
  if (!info || !t || t.fen !== line.value.fen || t.moves.join() !== line.value.moves.join()) return [];
  return info.lines.map((l) => ({
    score: scoreText(l.score, board.position.turn),
    san: sanLine(fen.value, l.pv.slice(0, 12)),
    first: l.pv[0],
  }));
});
const depth = computed(() => (lines.value.length ? evaluation.info?.depth : undefined));

const pasted = ref("");
const reading = computed(() => inspectPosition(pasted.value));
const pasteError = computed(() => ("error" in reading.value ? reading.value.error : undefined));
const pasteHint = computed(() =>
  "line" in reading.value
    ? (reading.value.note ?? `A ${reading.value.kind.toUpperCase()} with ${reading.value.line.moves.length} moves.`)
    : undefined,
);
function openPasted() {
  if (!("line" in reading.value)) return;
  board.openFree(reading.value.line.fen, reading.value.line.moves);
  pasted.value = "";
}
</script>

<template>
  <div class="explore">
    <section v-if="policy.allowed" class="engine" aria-label="Engine">
      <div class="engine-head">
        <ESwitch v-model="board.engineOn" label="Engine" description="Space turns it on and off." />
        <span v-if="board.engineOn && depth" class="depth num">depth {{ depth }}</span>
      </div>
      <ol v-if="board.engineOn" class="lines">
        <li v-for="(l, i) in lines" :key="i" class="line">
          <EButton
            size="sm"
            variant="quiet"
            class="line-btn"
            :aria-label="`Play ${l.san.split(' ')[1] ?? ''}`"
            :title="l.san"
            @click="l.first && board.exploreMove(l.first)"
          >
            <span class="line-score num">{{ l.score }}</span>
            <span class="line-pv num">{{ l.san }}</span>
          </EButton>
        </li>
        <li v-if="!lines.length" class="muted">{{ waiting }}</li>
      </ol>
    </section>
    <p v-else class="muted">{{ policy.reason }}</p>

    <p v-if="yours" class="muted">{{ yours }}</p>

    <section class="position" aria-label="Position">
      <CopyRow what="FEN" :text="fen" />
      <CopyRow what="PGN" :text="pgn" />
    </section>

    <ETextarea
      v-model="pasted"
      label="Open a position"
      placeholder="Paste a FEN or a PGN"
      :rows="2"
      size="sm"
      :error="pasteError"
      :hint="pasteHint"
    />
    <EActions>
      <EButton v-if="'line' in reading" size="sm" variant="outline" @click="openPasted">Open it</EButton>
      <EButton size="sm" variant="outline" @click="ui.startNewGame(line)">Play from here</EButton>
    </EActions>
  </div>
</template>

<style scoped>
.explore {
  display: grid;
  min-width: 0;
  gap: var(--e-gap-base);
}

.engine {
  display: grid;
  gap: var(--e-gap-tight);
}

.engine-head {
  display: flex;
  justify-content: space-between;
  align-items: flex-start;
  gap: var(--e-gap-snug);
}

.depth,
.muted {
  margin: 0;
  color: var(--e-muted);
  font: var(--e-type-small);
}

/* Room for the engine's lines before they arrive, so what is under them does not jump. */
.lines {
  display: grid;
  min-height: 5.625rem;
  align-content: start;
  grid-template-columns: minmax(0, 1fr);
  min-width: 0;
  margin: 0;
  padding: 0;
  list-style: none;
}

.line-btn {
  max-width: 100%;
  min-width: 0;
  overflow: hidden;
  justify-content: flex-start;
  gap: var(--e-gap-snug);
  width: 100%;
  height: 1.875rem;
  padding-inline: 0.25rem;
  text-align: start;
  text-decoration: none;
}

.line-score {
  flex: none;
  min-inline-size: 6ch;
  font: 700 0.8125rem/1 var(--e-font-mono);
}

.line-pv {
  min-width: 0;
  overflow: hidden;
  color: var(--e-muted);
  font: 0.8125rem/1.3 var(--e-font-mono);
  text-overflow: ellipsis;
  white-space: nowrap;
}

.position {
  display: grid;
  gap: var(--e-gap-snug);
  min-width: 0;
}
</style>
