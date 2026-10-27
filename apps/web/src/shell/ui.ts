import { ref } from "vue";
import { defineStore } from "pinia";
import type { Line } from "../board/position";

/** What the shell has open: the command bar, the shortcuts, the new-game dialog (and the position it starts from). */
export const useUi = defineStore("ui", () => {
  const palette = ref(false);
  const shortcuts = ref(false);
  const newGame = ref(false);
  /** A position to start the new game from ("Play from here", "Play this out"); `null` is the start. */
  const newGameFrom = ref<Line | null>(null);

  function startNewGame(from: Line | null = null) {
    newGameFrom.value = from;
    newGame.value = true;
  }

  return { palette, shortcuts, newGame, newGameFrom, startNewGame };
});
