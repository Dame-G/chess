/** The user's own Lichess games on the board: the fair-play lock, and what can be done about a game that is on. */
import { computed, ref } from "vue";
import { useQuery } from "@pinia/colada";
import { useToast } from "@eos-ui/vue";
import { api } from "../api/runner";
import { unwrap } from "../api/queries";
import { useRunnerEvent } from "../api/events";
import type { components } from "../api/schema";
import type { Match, MatchSummary, Side } from "./matchText";

type MeStatus = components["schemas"]["MeStatus"];

/** Whether a game is one of the user's own on Lichess (played here or imported), rather than the bot's or a local one. */
export const isMine = (m: Pick<Match, "lichess"> | Pick<MatchSummary, "lichess"> | undefined): boolean =>
  m?.lichess?.account === "me";

/**
 * Lichess's terms forbid help in a game: while one of the user's runs, the runner refuses analysis, the tablebase and the
 * explorer, and the board hides them. `locked` follows `fair_play_lock`; `known` is false until the runner has answered, so
 * nothing asks for analysis in the meantime. A runner that cannot say counts as not locked.
 */
export function useFairPlay() {
  const query = useQuery({
    key: ["lichess", "me"],
    query: () => unwrap(api.GET("/api/lichess/me")),
    refetchOnWindowFocus: true,
  });
  const pushed = ref<boolean>();
  useRunnerEvent("lichess.me.fair_play", (d) => {
    pushed.value = !!(d as { locked?: boolean }).locked;
  });
  useRunnerEvent("lichess.me.status", (d) => {
    pushed.value = !!(d as Partial<MeStatus>).fair_play_lock;
  });
  const locked = computed(() => pushed.value ?? query.state.value.data?.fair_play_lock ?? false);
  const known = computed(() => pushed.value !== undefined || query.state.value.status !== "pending");
  return { locked, known };
}

/** What a game's offers look like from the user's seat. */
export function offersOf(m: Match, me: Side) {
  const l = m.lichess;
  return {
    theirDraw: !!l?.draw_offer && l.draw_offer !== me,
    myDraw: l?.draw_offer === me,
    theirTakeback: !!l?.takeback_offer && l.takeback_offer !== me,
    myTakeback: l?.takeback_offer === me,
  };
}

/** The actions on a game of the user's, each a request to the runner that Lichess has to accept. Errors come up as toasts. */
export function useMyGameActions(id: () => number) {
  const toast = useToast();
  const busy = ref(false);

  async function run(call: () => Promise<unknown>) {
    busy.value = true;
    try {
      await call();
      return true;
    } catch (e) {
      toast.show({
        key: "lichess-game",
        title: "Lichess did not accept that",
        message: (e as Error).message,
        tone: "danger",
      });
      return false;
    } finally {
      busy.value = false;
    }
  }
  const path = () => ({ params: { path: { id: id() } } });
  return {
    busy,
    abort: () => run(() => unwrap(api.POST("/api/matches/{id}/abort", path()))),
    resign: () => run(() => unwrap(api.DELETE("/api/matches/{id}", path()))),
    /** Offers a draw, or accepts the opponent's offer: the runner decides which from the game. */
    draw: () => run(() => unwrap(api.POST("/api/matches/{id}/draw", path()))),
    declineDraw: () => run(() => unwrap(api.POST("/api/matches/{id}/decline-draw", path()))),
    takeback: () => run(() => unwrap(api.POST("/api/matches/{id}/takeback", path()))),
    declineTakeback: () => run(() => unwrap(api.POST("/api/matches/{id}/decline-takeback", path()))),
    claimVictory: () => run(() => unwrap(api.POST("/api/matches/{id}/claim-victory", path()))),
    claimDraw: () => run(() => unwrap(api.POST("/api/matches/{id}/claim-draw", path()))),
  };
}

/** Your games on Lichess, newest first: the Games tab lists them beside the rest. Match events keep the list current. */
export function useMyLichessGames() {
  return useQuery({
    key: ["matches", "mine"],
    query: () =>
      unwrap(api.GET("/api/matches", { params: { query: { source: "lichess", account: "me", limit: 50 } } })),
  });
}
