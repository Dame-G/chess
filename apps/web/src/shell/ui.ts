import { ref } from "vue";
import { defineStore } from "pinia";
import type { Line } from "../board/position";

/** What the shell has open: the command bar, the shortcuts, the new-game dialog (and the position it starts from), the Games rail. */
export const useUi = defineStore("ui", () => {
  const palette = ref(false);
  const shortcuts = ref(false);
  const newGame = ref(false);
  /** The Games rail is open over the board: a drawer from the left on a tablet, the Games tab on a phone. */
  const railOpen = ref(false);
  /** A position to start the new game from ("Play from here", "Play this out"); `null` is the start. */
  const newGameFrom = ref<Line | null>(null);

  function startNewGame(from: Line | null = null) {
    newGameFrom.value = from;
    newGame.value = true;
  }

  return { palette, shortcuts, newGame, newGameFrom, railOpen, startNewGame };
});
