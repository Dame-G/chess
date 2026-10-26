import { useRouter } from "vue-router";
import { useShortcuts } from "@eos-ui/vue";
import { useBoard } from "../board/board";
import { findPly, walksSide } from "../board/review";
import { useGames } from "../stores/games";
import { useUi } from "./ui";

/** Controls that use the arrow keys themselves: the board's steps leave them alone. */
const OWN_ARROWS =
  '[role="tablist"], [role="radiogroup"], [role="slider"], [role="listbox"], [role="menu"], [role="dialog"]';

/**
 * The app's keys, as docs/design.md lists them: ←/→, Home/End, F, Space (Explore's engine), X and Shift X (Review's
 * mistakes), N, G Y / G D / G S and `?`. ⌘K belongs to the command bar. Call once, in the app's root.
 */
export function useKeys() {
  const board = useBoard();
  const games = useGames();
  const ui = useUi();
  const router = useRouter();

  const arrows = (step: () => void) => (e: KeyboardEvent) => {
    const target = e.target as Element | null;
    if (target?.closest?.(OWN_ARROWS)) return;
    step();
  };

  function mistake(back: boolean) {
    const r = games.review;
    if (board.mode !== "review" || !r) return;
    const ply = findPly(r.plies, board.ply, walksSide(board.summary?.you ?? null), back);
    if (ply !== undefined) board.go(ply);
  }

  /** G starts a two-key sequence for a second. */
  let g = 0;
  const after = (to: string) => () => {
    if (Date.now() - g < 1000) void router.push(to);
    g = 0;
  };

  useShortcuts({
    left: arrows(() => board.go(board.cursor - 1)),
    right: arrows(() => board.go(board.cursor + 1)),
    home: arrows(() => board.go(0)),
    end: arrows(() => board.go(Infinity)),
    f: () => board.flip(),
    x: () => mistake(false),
    "shift+x": () => mistake(true),
    n: () => ui.startNewGame(),
    g: () => (g = Date.now()),
    y: after("/you"),
    d: after("/dameless"),
    s: after("/settings"),
    "?": () => (ui.shortcuts = true),
  });
  // Space presses a focused button, so it only turns the engine on and off when nothing pressable has focus.
  useShortcuts(
    {
      space: (e) => {
        const target = e.target as Element | null;
        if (!board.exploring || target?.closest?.("button, a, [role=button], [role=switch], [role=radio], [role=tab]"))
          return;
        e.preventDefault();
        board.engineOn = !board.engineOn;
      },
    },
    { preventDefault: false },
  );
}
