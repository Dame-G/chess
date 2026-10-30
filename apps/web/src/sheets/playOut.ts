import { ref } from "vue";
import { useRouter } from "vue-router";
import { useToast } from "@eos-ui/vue";
import type { WeakSpot } from "../api/types";
import { readClock } from "../shell/commands";
import { useLevels } from "../stores/levels";
import { useGames } from "../stores/games";
import { stored } from "../stored";
import { playOutSeat } from "./levels";

/** The key a weak spot goes by in a list. */
export const spotKey = (s: Pick<WeakSpot, "game" | "ply">): string => `${s.game}:${s.ply}`;

/** The board at a weak spot, in the game it came from. */
export const boardTo = (s: Pick<WeakSpot, "game" | "ply">) => ({
  path: "/",
  query: { game: String(s.game), ply: String(s.ply) },
});

/**
 * "Play this out": a local game against Dameless from the position before a mistake, You to move, at the level nearest
 * your rating so the position is practised against someone you can learn from. `starting` is the spot being started.
 */
export function usePlayOut() {
  const router = useRouter();
  const toast = useToast();
  const games = useGames();
  const ladderStore = useLevels();
  const clock = stored("new.clock", "5+3");
  const starting = ref<string | null>(null);

  async function playOut(spot: Pick<WeakSpot, "game" | "ply" | "fen">) {
    starting.value = spotKey(spot);
    const whiteToMove = spot.fen.split(" ")[1] !== "b";
    try {
      if (!ladderStore.levels) await ladderStore.loadLevels().catch(() => {});
      const seat = playOutSeat(ladderStore.levels);
      const s = await games.start({
        white: whiteToMove ? "you" : seat,
        black: whiteToMove ? seat : "you",
        time: readClock(clock.value) ?? { type: "clock", initial_ms: 300_000, increment_ms: 3000 },
        from: { type: "position", fen: spot.fen, moves: [] },
      });
      await router.push({ path: "/", query: { game: String(s.id) } });
    } catch (e) {
      toast.show({
        title: "The game did not start",
        message: e instanceof Error ? e.message : String(e),
        tone: "warning",
      });
    } finally {
      starting.value = null;
    }
  }

  return { starting, playOut };
}
