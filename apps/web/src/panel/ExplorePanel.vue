<script setup lang="ts">
import { computed, onMounted, ref } from "vue";
import { EButton } from "@eos-ui/vue/components/EButton";
import { ESwitch } from "@eos-ui/vue/components/ESwitch";
import { ETextarea } from "@eos-ui/vue/components/ETextarea";
import { buildingText } from "../format";
import { useBoard } from "../board/board";
import { inspectPosition, sanLine } from "../board/position";
import { useEvaluation } from "../stores/evaluation";
import { usePolicy } from "../stores/policy";
import { useRepertoire } from "../stores/repertoire";
import { useWork } from "../stores/work";
import { repertoireNote } from "../sheets/repertoire";
import { useUi } from "../shell/ui";
import UiEmpty from "../ui/UiEmpty.vue";
import UiScore from "../ui/UiScore.vue";
import UiToolbar from "../ui/UiToolbar.vue";
import PanelLock from "./PanelLock.vue";
import PanelMenu from "./PanelMenu.vue";

/**
 * Explore: free analysis from any position. Moves are played freely into a tree of variations; the engine's lines
 * follow the board and any move of one plays onto it; a FEN or PGN pasted opens on the board; Play from here starts a
 * game. The opening explorer and the tablebase come here next.
 */
const board = useBoard();
const evaluation = useEvaluation();
const policy = usePolicy();
const ui = useUi();
const work = useWork();
const repertoire = useRepertoire();

/** The first engine build can take a minute: say so, rather than a blank for as long. */
const waiting = computed(() => (work.building ? buildingText(work.building.title) : ""));

onMounted(() => {
  if (repertoire.available && !repertoire.sides.white) void repertoire.load();
});
const yours = computed(() => (repertoire.available ? repertoireNote(repertoire.sides, fen.value) : null));

const fen = computed(() => board.node.fen);
const line = computed(() => board.tree.line(board.node));

/**
 * The engine's lines, only while they are of the position on the board. Each is cut into its moves, so any move of it
 * can be clicked: the board plays the line as far as that move.
 */
const lines = computed(() => {
  const info = evaluation.info;
  const t = evaluation.target;
  if (!info || !t || t.fen !== line.value.fen || t.moves.join() !== line.value.moves.join()) return [];
  return info.lines.map((l) => {
    const pv = l.pv.slice(0, 12);
    let before = "";
    const moves = pv.map((_, k) => {
      const text = sanLine(fen.value, pv.slice(0, k + 1));
      const move = text.slice(before.length).trim();
      before = text;
      return move;
    });
    return { score: l.score, pv, moves, text: before };
  });
});
const depth = computed(() => (lines.value.length ? evaluation.info?.depth : undefined));

/** Plays the line as far as its `k`th move onto the board. */
function playLine(pv: string[], k: number) {
  for (const uci of pv.slice(0, k + 1)) board.exploreMove(uci);
}

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
/** Enter opens what is pasted; Shift+Enter is a new line, for a PGN typed by hand. */
function onEnter(e: KeyboardEvent) {
  if (e.shiftKey || !("line" in reading.value)) return;
  e.preventDefault();
  openPasted();
}
</script>

<template>
  <div class="explore">
    <PanelLock v-if="!policy.allowed" />
    <template v-else>
      <UiToolbar align="between" label="Engine">
        <span title="Space turns it on and off">
          <ESwitch v-model="board.engineOn" label="Engine" />
        </span>
        <span v-if="board.engineOn && depth" class="depth num">Depth {{ depth }}</span>
        <template #end>
          <EButton variant="outline" @click="ui.startNewGame(line)">Play from here</EButton>
          <PanelMenu />
        </template>
      </UiToolbar>

      <section v-if="board.engineOn" class="engine" aria-label="Engine lines">
        <ol v-if="lines.length" class="lines">
          <li v-for="(l, i) in lines" :key="i" class="line">
            <UiScore :score="l.score" :turn="board.position.turn" class="line-score" />
            <span class="line-pv num" :title="l.text">
              <button
                v-for="(m, k) in l.moves"
                :key="k"
                type="button"
                class="pvm"
                :aria-label="`Play ${m}`"
                :tabindex="k === 0 ? 0 : -1"
                @click="playLine(l.pv, k)"
              >
                {{ m }}
              </button>
            </span>
          </li>
        </ol>
        <template v-else-if="evaluation.error">
          <UiEmpty dense tone="error" title="The engine did not start." :body="evaluation.error" />
        </template>
        <div v-else class="lines lines--wait" role="status" aria-label="The engine is thinking">
          <span v-for="i in 3" :key="i" class="skel line-skel" />
          <p v-if="waiting" class="note">{{ waiting }}</p>
        </div>
      </section>

      <p v-if="yours" class="note">{{ yours }}</p>

      <ETextarea
        v-model="pasted"
        label="Paste a FEN or PGN"
        hide-label
        placeholder="Paste a FEN or PGN"
        :rows="1"
        auto-grow
        :error="pasteError"
        :hint="pasteHint"
        @keydown.enter="onEnter"
      />
      <UiToolbar v-if="'line' in reading" label="Open">
        <EButton variant="outline" @click="openPasted">Open</EButton>
      </UiToolbar>
    </template>
  </div>
</template>

<style scoped>
.explore {
  display: grid;
  min-width: 0;
  gap: var(--sp-3);
}

.depth,
.note {
  margin: 0;
  color: var(--c-ink-2);
  font: var(--t-meta);
}

.engine {
  min-width: 0;
}

/* Room for three lines before they arrive, so what is under them does not jump. */
.lines {
  display: grid;
  grid-template-columns: minmax(0, 1fr);
  min-width: 0;
  min-height: calc(3 * var(--h-row));
  margin: 0;
  padding: 0;
  align-content: start;
  list-style: none;
}

.line {
  display: grid;
  grid-template-columns: 4.5rem minmax(0, 1fr);
  align-items: center;
  min-height: var(--h-row);
  border-block-end: var(--e-stroke-hair) solid var(--c-line);
}

.line:last-child {
  border-block-end: 0;
}

.line-score {
  justify-self: end;
  padding-inline-end: var(--sp-3);
}

.line-pv {
  display: block;
  min-width: 0;
  overflow: hidden;
  color: var(--c-ink-2);
  font: var(--t-fig);
  text-overflow: ellipsis;
  white-space: nowrap;
}

.pvm {
  margin: 0;
  padding: var(--sp-1);
  border: 0;
  border-radius: var(--r-1);
  background: transparent;
  color: inherit;
  font: inherit;
  cursor: pointer;
}

.pvm:hover {
  background: var(--c-sunk);
  color: var(--c-ink);
}

.pvm:focus-visible {
  outline: var(--focus-ring);
}

.line-skel {
  height: calc(var(--h-row) - var(--sp-2));
  margin-block: var(--sp-1);
}
</style>
