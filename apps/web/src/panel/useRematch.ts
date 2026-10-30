import { computed, ref, watch } from "vue";
import { useRouter } from "vue-router";
import { useToast } from "@eos-ui/vue";
import { useBoard } from "../board/board";
import { opponentEngine, rematchRequest, strengthOf, type Strength } from "../board/rematch";
import { neighbour } from "../sheets/levels";
import { useEngines } from "../stores/engines";
import { useLevels } from "../stores/levels";
import { useGames } from "../stores/games";

/**
 * Playing the game on the board again: the same opponent and clock with the colours swapped, or one level of that
 * engine's ladder harder or easier (every engine on the clock has levels). Only a finished game of yours against an
 * engine played here can be played again.
 */
export function useRematch() {
  const board = useBoard();
  const games = useGames();
  const ladderStore = useLevels();
  const engines = useEngines();
  const router = useRouter();
  const toast = useToast();

  /** An engine has levels unless it plays a fixed budget; one not in the list (yet) is taken to have them. */
  const context = () => ({ hasLevels: (id: string) => engines.byId(id)?.nodes == null });
  const can = computed(() => !!board.game && rematchRequest(board.game, context()) !== null);
  /** The engine you played in the game on the board, and its ladder once read. */
  const engine = computed(() => (board.game ? opponentEngine(board.game) : null));
  const overview = computed(() => (engine.value ? ladderStore.of(engine.value) : null));
  watch(
    engine,
    (id) => {
      if (id && !ladderStore.of(id) && context().hasLevels(id)) void ladderStore.loadLevels(id).catch(() => {});
    },
    { immediate: true },
  );
  /** The strength the engine played at in the game on the board; `null` against one without levels. */
  const strength = computed<Strength | null>(() => (board.game ? strengthOf(board.game, context()) : null));
  const harder = computed(() => (strength.value === null ? null : neighbour(overview.value, strength.value, true)));
  const easier = computed(() => (strength.value === null ? null : neighbour(overview.value, strength.value, false)));

  const starting = ref(false);
  /** Starts the game; `to` plays one level up or down (a level's id, or `full`) instead of the same strength. */
  async function start(to?: Strength) {
    const g = board.game;
    if (!g || starting.value) return;
    starting.value = true;
    try {
      const request = rematchRequest(g, context(), to);
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

  return { can, engine, overview, strength, harder, easier, starting, start };
}
