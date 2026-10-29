<script setup lang="ts">
import { computed } from "vue";
import { PhDotsThree } from "@phosphor-icons/vue";
import { useToast } from "@eos-ui/vue";
import { EIconButton } from "@eos-ui/vue/components/EIconButton";
import { EMenu, type EMenuItem } from "@eos-ui/vue/components/EMenu";
import { resultOf, seatName } from "../format";
import { useBoard } from "../board/board";
import { INITIAL_FEN, toPgn } from "../board/position";

/**
 * The panel's one overflow menu, the same in every mode: copy the position's FEN or the line's PGN, and whatever the
 * mode adds (a rematch). What is copied is read when the row is chosen, so it is always what is on the board.
 */
const props = defineProps<{ extra?: readonly EMenuItem[] }>();
const board = useBoard();
const toast = useToast();

/** The PGN of what is on screen: the whole game, or Explore's line to the move on the board. */
function pgn(): string {
  if (board.exploring) return toPgn(board.tree.line(board.node));
  const g = board.game;
  if (!g) return "";
  const s = g.summary;
  return toPgn(
    { fen: g.start_fen ?? INITIAL_FEN, moves: g.moves.map((m) => m.uci) },
    { White: seatName(s.white), Black: seatName(s.black), Result: resultOf(s.outcome) ?? "*" },
  );
}
const fen = () => (board.exploring ? board.node.fen : (board.plies[board.ply]?.fen ?? INITIAL_FEN));

async function copy(what: string, text: string) {
  try {
    await navigator.clipboard.writeText(text);
    toast.show({ title: `${what} copied`, tone: "success" });
  } catch {
    toast.show({ title: `${what} could not be copied`, tone: "warning" });
  }
}

const items = computed<EMenuItem[]>(() => [
  ...(props.extra ?? []),
  ...(props.extra?.length ? [{ divider: true } as const] : []),
  { label: "Copy FEN", onClick: () => void copy("FEN", fen()) },
  { label: "Copy PGN", onClick: () => void copy("PGN", pgn()) },
]);
</script>

<template>
  <EMenu label="More" :items="items" side="bottom" align="end">
    <template #trigger>
      <EIconButton :icon="PhDotsThree" label="More" tooltip size="sm" variant="quiet" />
    </template>
  </EMenu>
</template>
