import { computed, ref } from "vue";
import { useRouter } from "vue-router";
import { useToast } from "@eos-ui/vue";
import { useBoard } from "../board/board";
import { rematchRequest, strengthOf, type Strength } from "../board/rematch";
import { neighbour } from "../sheets/levels";
import { useLevels } from "../stores/levels";
import { useGames } from "../stores/games";

/**
 * Playing the game on the board again: the same opponent and clock with the colours swapped, or one level harder or
 * easier. Only a finished game of yours against Dameless played here can be played again.
 */
export function useRematch() {
  const board = useBoard();
  const games = useGames();
  const ladderStore = useLevels();
  const router = useRouter();
  const toast = useToast();

  const context = () => ({ levels: ladderStore.levels });
  const can = computed(() => !!board.game && rematchRequest(board.game, context()) !== null);
  /** The strength Dameless played at in the game on the board. */
  const strength = computed<Strength | null>(() => (board.game ? strengthOf(board.game, ladderStore.levels) : null));
  const harder = computed(() => (strength.value === null ? null : neighbour(ladderStore.levels, strength.value, true)));
  const easier = computed(() =>
    strength.value === null ? null : neighbour(ladderStore.levels, strength.value, false),
  );

  const starting = ref(false);
  /** Starts the game; `to` plays one level up or down (a level's id, or `full`) instead of the same strength. */
  async function start(to?: Strength) {
    const g = board.game;
    if (!g || starting.value) return;
    starting.value = true;
    try {
      if (!ladderStore.levels) await ladderStore.loadLevels().catch(() => {});
      const request = rematchRequest(g, { ...context(), started: games.starts[String(g.summary.id)] ?? null }, to);
      if (!request) return;
      const made = await games.start(request);
      await router.push({ path: "/", query: { game: String(made.id) } });
    } catch (e) {
      toast.show({
        title: "The game did not start",
        message: e instanceof Error ? e.message : String(e),
        tone: "warning",
      });
    } finally {
      starting.value = false;
    }
  }

  return { can, strength, harder, easier, starting, start };
}
