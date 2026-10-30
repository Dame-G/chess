import { useRouter } from "vue-router";
import { useShortcuts, useToast } from "@eos-ui/vue";
import { useBoard } from "../board/board";
import { findPly, walksSide } from "../board/review";
import { useRematch } from "../panel/useRematch";
import { useGames } from "../stores/games";
import { useStartDrills } from "./drillFlow";
import { useUi } from "./ui";

/** Controls that use the arrow keys themselves: the board's steps leave them alone. */
const OWN_ARROWS =
  '[role="tablist"], [role="radiogroup"], [role="slider"], [role="listbox"], [role="menu"], [role="dialog"]';

/**
 * The app's keys, as docs/design.md lists them: ←/→, Home/End, F, Space (Explore's engine), X and Shift X (Review's
 * mistakes), T or ⌘Z (take back, in a game against Dameless), R (play the finished game again), N, D (today's drills),
 * G Y / G D / G S and `?`. ⌘K belongs to the command bar. Call once, in the app's root.
 */
export function useKeys() {
  const board = useBoard();
  const games = useGames();
  const ui = useUi();
  const router = useRouter();
  const startDrills = useStartDrills();
  const rematch = useRematch();
  const toast = useToast();

  /** Takes back your last move and the reply, in a local game that allows it (one with no clock). */
  async function takeBack() {
    const game = games.open;
    if (board.mode !== "play" || game?.summary.source.type !== "local" || !game.actions.includes("takeback")) return;
    try {
      await games.act("takeback");
    } catch (e) {
      toast.show({
        title: "That did not go through",
        message: e instanceof Error ? e.message : String(e),
        tone: "warning",
      });
    }
  }

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
  /** D alone starts today's drills; G then D goes to Dameless. */
  const drillsOrDameless = () => {
    if (Date.now() - g < 1000) {
      g = 0;
      void router.push("/dameless");
    } else void startDrills();
  };

  useShortcuts({
    left: arrows(() => board.step("back")),
    right: arrows(() => board.step("forward")),
    home: arrows(() => board.step("start")),
    end: arrows(() => board.step("end")),
    f: () => board.flip(),
    x: () => mistake(false),
    "shift+x": () => mistake(true),
    n: () => ui.startNewGame(),
    t: () => void takeBack(),
    "mod+z": () => void takeBack(),
    r: () => {
      if (board.mode === "review" && rematch.can.value) void rematch.start();
    },
    g: () => (g = Date.now()),
    y: after("/you"),
    d: drillsOrDameless,
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
