<script setup lang="ts">
import { computed, ref, watch } from "vue";
import { useRouter } from "vue-router";
import { PhCaretDown } from "@phosphor-icons/vue";
import { useToast } from "@eos-ui/vue";
import { EButton } from "@eos-ui/vue/components/EButton";
import type { EMenuItem } from "@eos-ui/vue/components/EMenu";
import { EProgress } from "@eos-ui/vue/components/EProgress";
import "../board/review.css";
import type { Classification, Side } from "../api/types";
import { progressText, reviewCost, seatName } from "../format";
import { useBoard } from "../board/board";
import Glyph from "../board/Glyph.vue";
import { sanLine } from "../board/position";
import { findPly, hasBetter, hint, markOf, markTip, SUMMARY_MARKS } from "../board/review";
import { rematchRequest } from "../board/rematch";
import { useDameless } from "../stores/dameless";
import { useGames } from "../stores/games";
import { usePolicy } from "../stores/policy";
import { useVersions } from "../stores/versions";
import { useWork } from "../stores/work";
import { useUi } from "../shell/ui";
import UiEmpty from "../ui/UiEmpty.vue";
import UiScore from "../ui/UiScore.vue";
import UiStat from "../ui/UiStat.vue";
import UiToolbar from "../ui/UiToolbar.vue";
import PanelLock from "./PanelLock.vue";
import PanelMenu from "./PanelMenu.vue";

/**
 * Review: a finished game read by the engine. Per side its accuracy, its average loss and the judgements it earned
 * (each a chip that jumps to the next such move), then the move on the board judged with the better move and its line.
 * Not reviewed yet: one button; being reviewed: how far it has got. Who played and how it ended are in the game card.
 */
const board = useBoard();
const games = useGames();
const policy = usePolicy();
const versions = useVersions();
const work = useWork();
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

/** The judgements a side earned worth a look: only those it has, in Lichess's order. */
function chips(side: Side) {
  const counts = review.value?.summary[side].counts;
  if (!counts) return [];
  return SUMMARY_MARKS.filter((c) => counts[c]).map((c) => ({ c, n: counts[c] ?? 0, mark: markOf(c) }));
}

const move = computed(() => (review.value && board.ply > 0 ? review.value.plies[board.ply - 1] : undefined));
const showLine = ref(false);
watch(move, () => (showLine.value = false));
const betterLine = computed(() => {
  const m = move.value;
  if (!m) return "";
  const before = board.plies[m.ply - 1]?.fen ?? "";
  return sanLine(before, m.best_pv);
});
const number = computed(() => {
  const m = move.value;
  return m ? `${Math.ceil(m.ply / 2)}${m.side === "white" ? "." : "…"} ${m.san}` : "";
});

function jump(side: Side, c: Classification) {
  const r = review.value;
  if (!r) return;
  const ply = findPly(r.plies, board.ply, (p) => p.side === side && p.classification === c);
  if (ply !== undefined) board.go(ply);
}

/** A local game against Dameless from the position on the board. */
function playFromHere() {
  const g = board.game;
  if (!g) return;
  ui.startNewGame({ fen: board.plies[0]?.fen ?? "", moves: g.moves.slice(0, board.ply).map((m) => m.uci) });
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
const extra = computed<EMenuItem[]>(() =>
  canRematch.value ? [{ label: "Rematch", disabled: starting.value, onClick: () => void rematch() }] : [],
);

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

/** The work that is reading this game now, if any. */
const reading = computed(() =>
  [...work.running, ...work.queued].find(
    (w) => w.kind === "review" && w.subject.type === "game" && w.subject.id === board.gameId,
  ),
);
const fraction = computed(() => {
  const p = reading.value?.progress;
  return p?.total ? Math.min(100, (p.done / p.total) * 100) : undefined;
});
async function run(f: (id: number) => Promise<unknown>, id: number) {
  try {
    await f(id);
  } catch (e) {
    toast.show({
      title: "That did not go through",
      message: e instanceof Error ? e.message : String(e),
      tone: "warning",
    });
  }
}
const empty = computed(() => (board.game?.moves.length ?? 0) === 0 && !games.openLoading);

const fmt = (n: number | null | undefined) => (n == null ? null : n.toFixed(1));
</script>

<template>
  <div class="review">
    <PanelLock v-if="!policy.allowed" />

    <div v-else-if="games.reviewLoading && !review" class="wait" role="status" aria-label="Loading the review">
      <span class="skel wait-sum" />
      <span class="skel wait-move" />
    </div>

    <template v-else-if="review">
      <section class="sum" aria-label="Review">
        <div v-for="side in sides" :key="side" class="col">
          <p class="who">
            <span class="pip" :class="`pip--${side}`" aria-hidden="true" />
            <span class="truncate">{{ name(side) }}</span>
          </p>
          <div class="sum-figs">
            <UiStat label="Accuracy" hint="accuracy" size="xl" :value="fmt(review.summary[side].accuracy)" />
            <UiStat label="Avg. loss" hint="Average centipawn loss" :value="review.summary[side].acpl" :digits="0" />
          </div>
          <ul v-if="chips(side).length" class="chips" :aria-label="`${name(side)}: judgements`">
            <li v-for="x in chips(side)" :key="x.c">
              <button
                type="button"
                class="chip"
                :class="[`rv-${x.c}`, { 'is-on': move?.side === side && move.classification === x.c }]"
                :title="`${x.mark.label}: ${markTip(x.c)}`"
                :aria-label="`${name(side)}: ${x.n} ${x.mark.label}, go to the next`"
                @click="jump(side, x.c)"
              >
                <Glyph :of="x.c" badge />
                <span class="chip-n num">{{ x.n }}</span>
              </button>
            </li>
          </ul>
        </div>
      </section>

      <section
        class="move"
        :class="move ? `rv-${move.classification}` : undefined"
        aria-label="This move"
        aria-live="polite"
      >
        <template v-if="move">
          <p class="mline" :title="hint(move)">
            <span class="ply num">{{ number }}</span>
            <Glyph :of="move.classification" badge class="mglyph" />
            <UiScore :score="move.eval_after" :turn="move.side === 'white' ? 'black' : 'white'" class="mscore" />
          </p>
          <p class="better">
            <span class="mlabel">{{ markOf(move.classification).label }}</span>
            <template v-if="hasBetter(move)">
              <span class="sep" aria-hidden="true">·</span>
              <span
                >Better: <strong class="san num">{{ move.best_san }}</strong></span
              >
              <button
                type="button"
                class="toggle"
                :class="{ 'is-open': showLine }"
                :aria-expanded="showLine"
                @click="showLine = !showLine"
              >
                Line <PhCaretDown class="toggle-icon" aria-hidden="true" />
              </button>
            </template>
          </p>
          <p v-if="showLine && hasBetter(move)" class="pv num">{{ betterLine }}</p>
        </template>
        <p v-else class="hint">Step through the moves to see each one judged. X goes to the next mistake.</p>
      </section>

      <UiToolbar align="between" label="Review actions">
        <EButton variant="outline" @click="playFromHere">Play from here</EButton>
        <template #end><PanelMenu :extra="extra" /></template>
      </UiToolbar>
    </template>

    <template v-else-if="empty">
      <UiEmpty dense title="No moves were played." body="There is nothing to review." />
      <UiToolbar align="between" label="Game">
        <template #end><PanelMenu :extra="extra" /></template>
      </UiToolbar>
    </template>

    <div v-else-if="reading || asked" class="reading" role="status">
      <EProgress
        :value="fraction"
        :label="
          reading?.status === 'paused'
            ? `Review paused${reading.paused_for ? `: ${reading.paused_for}` : ''}`
            : 'Reviewing this game'
        "
        :value-text="progressText(reading?.progress) ?? undefined"
      />
      <UiToolbar v-if="reading" label="Review work">
        <EButton v-if="reading.status === 'paused'" variant="outline" @click="run(work.resume, reading.id)">
          Resume
        </EButton>
        <EButton v-else variant="outline" @click="run(work.pause, reading.id)">Pause</EButton>
      </UiToolbar>
    </div>

    <template v-else>
      <UiEmpty dense title="This game hasn't been reviewed." :body="`Takes ${reviewCost(1)}, in the background.`">
        <template #actions>
          <EButton variant="primary" :loading="asking" @click="requestReview">Review it</EButton>
        </template>
      </UiEmpty>
      <UiToolbar align="between" label="Game">
        <template #end><PanelMenu :extra="extra" /></template>
      </UiToolbar>
    </template>
  </div>
</template>

<style scoped>
.review {
  display: grid;
  gap: var(--sp-3);
  min-width: 0;
}

/* The two sides sit in the columns the game card puts them in: White's at the start, Black's at the end. */
.sum {
  display: grid;
  grid-template-columns: repeat(2, minmax(0, 1fr));
  gap: var(--sp-3);
}

.col {
  display: grid;
  gap: var(--sp-2);
  align-content: start;
  min-width: 0;
}

.who {
  display: flex;
  align-items: center;
  gap: var(--sp-2);
  margin: 0;
  color: var(--c-ink-2);
  font: var(--t-meta);
}

.pip {
  flex: none;
  box-sizing: border-box;
  width: var(--sp-2);
  height: var(--sp-2);
  border: var(--e-stroke-hair) solid var(--c-line-strong);
}

.pip--white {
  background: var(--c-pip-white);
}

.pip--black {
  background: var(--c-pip-black);
}

.sum-figs {
  display: flex;
  flex-wrap: wrap;
  align-items: flex-end;
  gap: var(--sp-1) var(--sp-4);
}

.chips {
  display: flex;
  flex-wrap: wrap;
  gap: var(--sp-1);
  margin: 0;
  padding: 0;
  list-style: none;
}

.chip {
  display: inline-flex;
  align-items: center;
  gap: var(--sp-1);
  box-sizing: border-box;
  min-height: var(--h-move);
  padding: 0 var(--sp-2) 0 var(--sp-1);
  border: var(--e-stroke-hair) solid transparent;
  border-radius: var(--r-1);
  background: transparent;
  color: var(--c-ink);
  font: var(--t-meta);
  cursor: pointer;
}

.chip:hover {
  background: var(--c-raise);
}

.chip.is-on {
  border-color: var(--rv);
  background: var(--rv-wash);
}

.chip:focus-visible {
  outline: var(--focus-ring);
}

.chip-n {
  color: var(--rv);
  font: var(--t-fig);
  font-weight: 700;
}

/* The current move: one card whose height does not change as the board steps, so nothing under it moves. */
.move {
  display: grid;
  gap: var(--sp-1);
  align-content: start;
  min-height: calc(2 * var(--h-row));
  padding: var(--sp-2) var(--sp-3);
  border-radius: var(--r-1);
  background: var(--c-raise);
  box-shadow: inset var(--e-stroke-focus) 0 0 var(--rv, var(--c-line-strong));
}

.mline,
.better {
  display: flex;
  align-items: center;
  gap: var(--sp-2);
  min-height: var(--h-move);
  margin: 0;
}

.ply {
  font: var(--t-fig);
  font-weight: 700;
}

.mlabel {
  color: var(--rv, var(--c-ink-2));
  font: var(--t-body-strong);
}

.mglyph {
  margin: 0;
}

.mscore {
  margin-inline-start: auto;
}

.better {
  color: var(--c-ink-2);
}

.sep {
  color: var(--c-ink-3);
}

.san {
  color: var(--c-ink);
  font: var(--t-fig);
  font-weight: 700;
}

.toggle {
  margin-inline-start: auto;
  display: inline-flex;
  align-items: center;
  gap: var(--sp-1);
  min-height: var(--h-move);
  padding: 0 var(--sp-2);
  border: 0;
  border-radius: var(--r-1);
  background: transparent;
  color: var(--c-ink-2);
  font: var(--t-body);
  cursor: pointer;
}

.toggle:hover {
  background: var(--c-sunk);
  color: var(--c-ink);
}

.toggle:focus-visible {
  outline: var(--focus-ring);
}

.toggle-icon {
  transition: transform var(--m-fast) var(--m-ease);
}

.is-open .toggle-icon {
  transform: rotate(180deg);
}

.pv {
  margin: 0;
  color: var(--c-ink);
  font: var(--t-fig);
  overflow-wrap: anywhere;
}

.hint {
  margin: 0;
  color: var(--c-ink-2);
  font: var(--t-body);
}

.wait {
  display: grid;
  gap: var(--sp-3);
}

.wait-sum {
  height: calc(4 * var(--h-row));
}

.wait-move {
  height: calc(2 * var(--h-row));
}

.reading {
  display: grid;
  gap: var(--sp-2);
}

@media (prefers-reduced-motion: reduce) {
  .toggle-icon {
    transition: none;
  }
}
</style>
