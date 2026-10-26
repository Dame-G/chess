<script setup lang="ts">
/**
 * The endgame tablebase: the verdict for the side to move and every move with what it earns, best first. Shown only
 * for 7 pieces or fewer; with more it renders nothing and asks nothing.
 *
 * Props
 * - `fen`: the position on the board.
 * - `wait`: the debounce in ms (default 200).
 *
 * Emits
 * - `play(uci)`: a move row was clicked, in standard UCI (castling as `e1g1`).
 *
 * States: loading (previous rows dimmed), locked by fair play and failed. It needs no login. The router is needed
 * only for the notice's link.
 */
import { computed } from "vue";
import { EButton } from "@eos-ui/vue/components/EButton";
import { ESkeleton } from "@eos-ui/vue/components/ESkeleton";
import ExplorerNotice from "./ExplorerNotice.vue";
import { MAX_PIECES, pieceCount, useTablebase, type Category } from "./useTablebase";

const props = withDefaults(defineProps<{ fen: string; wait?: number }>(), { wait: 200 });
const emit = defineEmits<{ play: [uci: string] }>();

const shown = computed(() => pieceCount(props.fen) <= MAX_PIECES);
const { data, state, rows, message } = useTablebase(() => props.fen, props.wait);
const failed = computed(() => (["login", "locked", "limited", "error"] as const).find((s) => s === state.value));
const busy = computed(() => state.value === "loading");

const words: Record<Category, string> = {
  win: "Win",
  "syzygy-win": "Win",
  "maybe-win": "Probable win",
  "cursed-win": "Cursed win",
  draw: "Draw",
  unknown: "Unknown",
  "blessed-loss": "Blessed loss",
  "maybe-loss": "Probable loss",
  "syzygy-loss": "Loss",
  loss: "Loss",
};
const tone = (c: Category) =>
  c.includes("win")
    ? c === "cursed-win"
      ? "meh"
      : "win"
    : c.includes("loss")
      ? c === "blessed-loss"
        ? "meh"
        : "loss"
      : "meh";

/** Mate in N moves, from plies. */
const mateIn = (dtm: number) => Math.ceil(Math.abs(dtm) / 2);

const verdict = computed(() => {
  const a = data.value;
  if (!a) return undefined;
  const category = a.category;
  let detail = "";
  if (a.checkmate) detail = "Checkmate";
  else if (a.stalemate) detail = "Stalemate";
  else if (a.insufficient_material) detail = "Insufficient material";
  else if (a.dtm && (category === "win" || category === "loss")) detail = `Mate in ${mateIn(a.dtm)}`;
  else if (category === "cursed-win") detail = "Drawn by the 50-move rule";
  else if (category === "blessed-loss") detail = "Saved by the 50-move rule";
  return { category, detail, dtz: a.dtz == null ? undefined : Math.abs(a.dtz) };
});

function figure(r: { dtm?: number; dtz?: number; zeroing: boolean }) {
  if (r.dtm) return `M${mateIn(r.dtm)}`;
  if (r.dtz) return `DTZ ${r.dtz}`;
  return "";
}
</script>

<template>
  <section v-if="shown" class="tb" aria-label="Endgame tablebase" :aria-busy="busy">
    <ExplorerNotice v-if="failed" :state="failed" :message="message" what="tablebase" />
    <ESkeleton v-else-if="!data" :lines="3" label="Loading the tablebase" class="pad" />
    <div v-else :class="{ dim: busy }">
      <p v-if="verdict" class="verdict">
        <strong :class="tone(verdict.category)">{{ words[verdict.category] }}</strong>
        <span v-if="verdict.detail" class="detail">{{ verdict.detail }}</span>
        <span v-if="verdict.dtz !== undefined && !verdict.detail" class="detail num">DTZ {{ verdict.dtz }}</span>
        <span class="side">for the side to move</span>
      </p>
      <ul v-if="rows.length" class="moves" aria-label="Tablebase moves">
        <li v-for="r in rows" :key="r.uci">
          <EButton size="sm" variant="quiet" class="row" @click="emit('play', r.uci)">
            <span class="san">{{ r.san }}</span>
            <span class="out" :class="tone(r.outcome)">{{ words[r.outcome] }}</span>
            <span class="fig num">{{ figure(r) }}</span>
          </EButton>
        </li>
      </ul>
    </div>
  </section>
</template>

<style scoped>
.tb {
  display: grid;
  min-width: 0;
}

.pad {
  padding: var(--e-gap-base) var(--ws-pad);
}

.dim {
  opacity: 0.5;
}

.verdict {
  display: flex;
  flex-wrap: wrap;
  align-items: baseline;
  gap: var(--e-gap-snug);
  margin: 0;
  padding: var(--e-gap-snug) var(--ws-pad);
}

.detail,
.side {
  color: var(--e-muted);
  font: var(--e-type-small);
}

.moves {
  display: grid;
  margin: 0;
  padding: 0;
  list-style: none;
}

.row {
  display: grid;
  width: 100%;
  min-height: 1.75rem;
  grid-template-columns: 4rem minmax(0, 1fr) auto;
  gap: var(--e-gap-snug);
  padding-block: 0;
  padding-inline: var(--ws-pad);
  font-weight: 400;
  place-items: center start;
  text-align: start;
}

.san {
  font-family: var(--e-font-mono);
  font-weight: 600;
}

.fig {
  color: var(--e-muted);
  font: var(--e-type-small);
  font-family: var(--e-font-mono);
}

.win {
  color: var(--e-success);
}

.loss {
  color: var(--e-danger);
}

.meh {
  color: var(--e-muted);
}
</style>
