import { computed, watch } from "vue";
import { useRoute, useRouter, type LocationQuery } from "vue-router";
import { useDrills } from "../stores/drills";
import { useGames } from "../stores/games";
import { useBoard } from "./board";
import { firstGame } from "./modes";
import { INITIAL_FEN, isValidFen } from "./position";

/**
 * The address is the board's state: `/?game=<id>&ply=<n>` (no ply while following a live game) or `/?fen=…` for a free
 * position, or `/?drill=<id>` for a drill. Reading it opens what it names; moving on the board rewrites it in place. With nothing named, what opens
 * first follows docs/design.md: a live game, else your last finished game, else the start position in Explore.
 * Call once, in the app's root.
 */
export function useBoardAddress() {
  const route = useRoute();
  const router = useRouter();
  const board = useBoard();
  const games = useGames();
  const drills = useDrills();

  const address = computed(() => board.address);

  const same = (q: LocationQuery, a: Record<string, string>) =>
    Object.keys(a).length === Object.keys(q).filter((k) => q[k] !== undefined).length &&
    Object.entries(a).every(([k, v]) => q[k] === v);

  let chosen = false;
  async function read(q: LocationQuery) {
    const drill = Number(q["drill"]);
    if (Number.isInteger(drill) && drill > 0) {
      if (drills.active?.id === drill || (await drills.open(drill))) {
        chosen = true;
        return;
      }
    } else if (drills.active) drills.leave();
    const game = Number(q["game"]);
    const ply = q["ply"] === undefined ? undefined : Number(q["ply"]);
    if (Number.isInteger(game) && game > 0) {
      chosen = true;
      if (board.gameId !== game) await board.openGame(game, ply);
      else if (ply !== undefined && ply !== board.ply) board.go(ply);
      return;
    }
    const fen = typeof q["fen"] === "string" ? q["fen"] : undefined;
    if (fen && isValidFen(fen)) {
      chosen = true;
      if (board.gameId !== null || board.node.fen !== fen) board.openFree(fen);
      return;
    }
    if (chosen) {
      // An empty address after something was open: the start position.
      if (board.gameId !== null || board.node.fen !== INITIAL_FEN) board.openFree();
      return;
    }
    if (!games.loaded) return;
    chosen = true;
    const first = firstGame(games.list);
    if (first) await board.openGame(first.id);
    else board.openFree();
  }

  watch(
    () => (route.path === "/" ? route.query : null),
    (q) => {
      if (q && !same(q, address.value)) void read(q).catch(() => {});
    },
    { immediate: true },
  );
  // The first choice waits for the games list.
  watch(
    () => games.loaded,
    (loaded) => {
      if (loaded && !chosen && route.path === "/") void read(route.query).catch(() => {});
    },
  );
  watch(address, (a) => {
    if (route.path === "/" && chosen && !same(route.query, a)) void router.replace({ path: "/", query: a });
  });
}
