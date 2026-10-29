import { computed, ref, shallowRef } from "vue";
import { defineStore } from "pinia";
import { client, ok, ApiError } from "../api/client";
import { stream } from "../api/stream";
import type {
  Game,
  GameChanged,
  GameId,
  GameSummary,
  GameThinking,
  NewGameRequest,
  PlayerKey,
  Review,
  SeatAction,
  StartFrom,
} from "../api/types";
import { stored } from "../stored";
import { listen, reloadOnReconnect } from "./listen";

/** Live games first (they are pinned), then newest first. */
export function byRailOrder(a: GameSummary, b: GameSummary): number {
  const live = (g: GameSummary) => (g.status === "live" || g.status === "starting" ? 1 : 0);
  return live(b) - live(a) || b.started_at.localeCompare(a.started_at) || b.id - a.id;
}

/**
 * Every game the app knows, as summaries by id, and the one open on the board in full: its moves, clocks, offers,
 * what You may do, the engine's thinking and its review. Loaded with one GET and kept current by the `game.*` events;
 * the open game's detail comes on its own topic, `game:<id>`.
 */
export const useGames = defineStore("games", () => {
  const summaries = ref<Record<GameId, GameSummary>>({});
  const loaded = ref(false);
  const error = ref<string | null>(null);

  const open = ref<Game | null>(null);
  /** When the open game's clocks were last set, by `Date.now()`: the side to move's clock runs from there. */
  const clocksAt = ref(0);
  const thinking = shallowRef<GameThinking | null>(null);
  const opponentGone = ref<GameChanged["opponent_gone"]>(null);
  const review = shallowRef<Review | null>(null);
  /** Whether the open game's review is being fetched. */
  const reviewLoading = ref(false);
  const openLoading = ref(false);
  /** The game last asked to open: a slower answer for an earlier one is dropped. */
  let wanted: GameId | null = null;
  /** Games this tab started, which need no "your game started" toast. */
  const startedHere = new Set<GameId>();
  /** How the games this browser started began (a book line or a position), for a rematch to begin the same way. */
  const starts = stored<Record<string, StartFrom | null>>("game.starts", {});

  const list = computed(() => Object.values(summaries.value).sort(byRailOrder));

  async function load() {
    try {
      const games = await ok(client.GET("/api/games", { params: { query: { limit: 300 } } }));
      const next: Record<GameId, GameSummary> = {};
      for (const g of games) next[g.id] = g;
      summaries.value = next;
      error.value = null;
    } catch (e) {
      error.value = e instanceof Error ? e.message : String(e);
    } finally {
      loaded.value = true;
    }
    if (wanted !== null) await openGame(wanted, true);
  }

  async function fetchReview(id: GameId) {
    reviewLoading.value = true;
    try {
      const r: Review = await ok(client.GET("/api/games/{id}/review", { params: { path: { id } } }));
      if (open.value?.summary.id === id) review.value = r;
    } catch (e) {
      if (!(e instanceof ApiError && e.status === 404)) throw e;
      if (open.value?.summary.id === id) review.value = null;
    } finally {
      reviewLoading.value = false;
    }
  }

  /** Opens a game on the board: fetches it, follows its topic, and fetches its review when it is over. */
  async function openGame(id: GameId, again = false) {
    const before = wanted;
    if (before === id && open.value?.summary.id === id && !again) return;
    if (before !== null && before !== id) stream.unfollow(`game:${before}`);
    wanted = id;
    if (open.value?.summary.id !== id) {
      thinking.value = null;
      review.value = null;
      opponentGone.value = null;
    }
    stream.follow(`game:${id}`);
    openLoading.value = true;
    try {
      const game: Game = await ok(client.GET("/api/games/{id}", { params: { path: { id } } }));
      // A later call asked for another game while this one was on its way.
      if (wanted !== id) return;
      open.value = game;
      clocksAt.value = Date.now();
      summaries.value[id] = game.summary;
      if (game.summary.status === "finished" || game.summary.status === "aborted") await fetchReview(id);
    } finally {
      openLoading.value = false;
    }
  }

  function closeGame() {
    if (wanted !== null) stream.unfollow(`game:${wanted}`);
    wanted = null;
    open.value = null;
    review.value = null;
    thinking.value = null;
    opponentGone.value = null;
  }

  /** One of the seat actions (`move`, `offer_draw`, `resign`…) in the open game, as You; answers the game after it. */
  async function act(action: SeatAction, body?: { uci: string }): Promise<Game | undefined> {
    const id = open.value?.summary.id;
    if (id === undefined) return undefined;
    return await ok(
      client.POST("/api/games/{id}/actions/{action}", { params: { path: { id, action } }, body: body ?? {} }),
    );
  }

  /** Asks for games to be reviewed: work, whose results arrive as `review.upserted`. */
  async function requestReview(ids: GameId[]) {
    await ok(client.POST("/api/reviews", { body: { games: ids } }));
  }

  /** Reviews a player's newest games (`you`: your last `limit`). */
  async function reviewPlayer(player: PlayerKey, limit: number) {
    await ok(client.POST("/api/reviews", { body: { player, limit } }));
  }

  /** Reviews every game of the player's not reviewed yet, newest first, in the background (below ideas). */
  async function reviewAll(player: PlayerKey) {
    await ok(client.POST("/api/reviews", { body: { player, background: true } }));
  }

  /** Starts a local game (`POST /api/games`) and answers its summary, already in the list. */
  async function start(request: NewGameRequest): Promise<GameSummary> {
    const game = await ok(client.POST("/api/games", { body: request }));
    const s = game.summary;
    summaries.value[s.id] = s;
    startedHere.add(s.id);
    starts.value = { ...Object.fromEntries(Object.entries(starts.value).slice(-99)), [s.id]: request.from ?? null };
    return s;
  }

  const isOpen = (id: GameId) => open.value?.summary.id === id;

  listen("game.upserted", (s) => {
    summaries.value[s.id] = s;
    if (open.value && isOpen(s.id)) open.value.summary = s;
  });
  listen("game.moved", (d) => {
    const s = summaries.value[d.id];
    if (s && s.plies < d.ply) s.plies = d.ply;
    const g = open.value;
    if (!g || !isOpen(d.id)) return;
    // A move at a ply already held replaces it and whatever followed (a replay after a resume); a gap is refetched.
    if (d.ply > g.moves.length + 1) {
      void openGame(d.id, true);
      return;
    }
    g.moves.splice(d.ply - 1, g.moves.length, d.move);
    g.summary.plies = g.moves.length;
    if (d.clocks) {
      g.clocks = d.clocks;
      clocksAt.value = Date.now();
    }
    if (thinking.value && thinking.value.ply <= d.ply) thinking.value = null;
  });
  listen("game.undone", (d) => {
    const s = summaries.value[d.id];
    if (s) s.plies = d.plies;
    const g = open.value;
    if (!g || !isOpen(d.id)) return;
    g.moves.splice(d.plies);
    g.summary.plies = g.moves.length;
    thinking.value = null;
  });
  listen("game.changed", (d) => {
    const g = open.value;
    if (!g || !isOpen(d.id)) return;
    g.offers = d.offers;
    g.actions = d.actions;
    if (d.clocks) {
      g.clocks = d.clocks;
      clocksAt.value = Date.now();
    }
    opponentGone.value = d.opponent_gone ?? null;
  });
  listen("game.thinking", (d) => {
    if (!isOpen(d.id)) return;
    // Only the move being chosen now: a late line for a move already played is stale.
    if (open.value && d.ply !== open.value.moves.length + 1) return;
    thinking.value = d;
  });
  listen("review.upserted", (d) => {
    const s = summaries.value[d.game];
    const { white, black } = d.accuracy;
    if (s) s.accuracy = white != null && black != null ? { white, black } : null;
    if (isOpen(d.game)) void fetchReview(d.game).catch(() => {});
  });
  reloadOnReconnect(load);

  return {
    summaries,
    list,
    loaded,
    error,
    open,
    openLoading,
    clocksAt,
    thinking,
    opponentGone,
    review,
    reviewLoading,
    load,
    openGame,
    closeGame,
    act,
    requestReview,
    reviewPlayer,
    reviewAll,
    start,
    startedHere,
    starts,
  };
});
