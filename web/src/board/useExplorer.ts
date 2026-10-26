import { computed, onScopeDispose, ref, shallowRef, toValue, watch, type MaybeRefOrGetter } from "vue";
import { Chess } from "chessops/chess";
import { parseFen } from "chessops/fen";
import { parseUci } from "chessops/util";
import { api } from "../api/runner";
import { uciOf } from "./position";

/** What a lookup is doing. `login` and `locked` are the two 409s, told apart by the runner's words. */
export type LookupState = "idle" | "loading" | "ready" | "login" | "locked" | "limited" | "error";

/** The 409s: not logged in, or the fair-play lock while a game runs on Lichess. */
function classify409(message: string): "login" | "locked" {
  return /needs a login|connect your lichess|log in/i.test(message) ? "login" : "locked";
}

type Query = Record<string, string | number | undefined>;
export interface LookupRequest {
  path: "/api/lichess/explorer" | "/api/lichess/tablebase";
  query: Query;
}

/**
 * Asks the runner for `request` whenever it changes: waits `wait` ms so stepping quickly through moves asks once,
 * and aborts the answer it no longer wants. `undefined` asks nothing. The runner caches, so going back is free.
 * The last answer stays in `data` while the next one loads, so a panel can dim it instead of flashing.
 */
export function useLookup<T>(request: MaybeRefOrGetter<LookupRequest | undefined>, wait = 200) {
  const data = shallowRef<T>();
  const state = ref<LookupState>("idle");
  const message = ref("");
  let timer: ReturnType<typeof setTimeout> | undefined;
  let abort: AbortController | undefined;

  async function run(req: LookupRequest) {
    abort?.abort();
    const mine = (abort = new AbortController());
    state.value = "loading";
    try {
      const {
        data: body,
        error,
        response,
      } = await api.GET(req.path, { params: { query: req.query as never }, signal: mine.signal });
      if (mine.signal.aborted) return;
      if (response.ok) {
        data.value = body as T;
        state.value = "ready";
        return;
      }
      message.value = (error as { error?: string } | undefined)?.error ?? `${response.status} ${response.statusText}`;
      if (response.status === 409) state.value = classify409(message.value);
      else if (response.status === 429) state.value = "limited";
      else state.value = "error";
    } catch (e) {
      if (mine.signal.aborted) return;
      message.value = e instanceof Error ? e.message : String(e);
      state.value = "error";
    }
  }

  watch(
    () => {
      const r = toValue(request);
      return r ? JSON.stringify(r) : "";
    },
    (key) => {
      clearTimeout(timer);
      if (!key) {
        abort?.abort();
        state.value = "idle";
        return;
      }
      const req = JSON.parse(key) as LookupRequest;
      // The first lookup goes at once; later ones wait for the stepping to stop.
      if (state.value === "idle") void run(req);
      else {
        state.value = "loading";
        timer = setTimeout(() => void run(req), wait);
      }
    },
    { immediate: true },
  );
  onScopeDispose(() => {
    clearTimeout(timer);
    abort?.abort();
  });
  return { data, state, message };
}

/** A UCI move in standard castling (`e1g1`), from Lichess's king-takes-rook (`e1h1`), for the position it is played in. */
export function standardUci(fen: string, uci: string): string {
  const move = parseUci(uci);
  const setup = parseFen(fen);
  if (!move || setup.isErr) return uci;
  const pos = Chess.fromSetup(setup.value);
  if (pos.isErr) return uci;
  return uciOf(pos.value, move);
}

export interface ExplorerGame {
  id: string;
  winner: "white" | "black" | null;
  white: { name: string; rating?: number };
  black: { name: string; rating?: number };
  year?: number | null;
  month?: string | null;
  uci?: string;
}
interface ExplorerMove {
  uci: string;
  san: string;
  white: number;
  draws: number;
  black: number;
  averageRating?: number | null;
  averageOpponentRating?: number | null;
  performance?: number | null;
}
export interface ExplorerAnswer {
  white: number;
  draws: number;
  black: number;
  moves: ExplorerMove[];
  topGames?: ExplorerGame[];
  recentGames?: ExplorerGame[];
  opening?: { eco: string; name: string } | null;
}

export type Database = "lichess" | "masters" | "player";
export interface ExplorerFilters {
  db: Database;
  speeds: string[];
  ratings: string[];
  /** The player's username for `db=player`. */
  player?: string;
  color?: "white" | "black";
}

/** The request for a position, or nothing where there is nothing to ask (the player's games without a username). */
function explorerRequest(fen: string, f: ExplorerFilters): LookupRequest | undefined {
  if (!fen || (f.db === "player" && !f.player)) return undefined;
  const query: Query = { db: f.db, fen, moves: 12 };
  if (f.db === "lichess") {
    query.top_games = 4;
    query.recent_games = 4;
    if (f.speeds.length) query.speeds = f.speeds.join(",");
    if (f.ratings.length) query.ratings = f.ratings.join(",");
  } else if (f.db === "masters") {
    query.top_games = 15;
  } else {
    query.player = f.player;
    query.color = f.color ?? "white";
    query.recent_games = 8;
    if (f.speeds.length) query.speeds = f.speeds.join(",");
  }
  return { path: "/api/lichess/explorer", query };
}

/** Shares of a row that add up to 100, for the bar and its labels. */
export function shares(white: number, draws: number, black: number) {
  const total = white + draws + black;
  if (!total) return { total: 0, white: 0, draws: 0, black: 0 };
  const w = Math.round((white / total) * 100);
  const b = Math.round((black / total) * 100);
  return { total, white: w, draws: Math.max(0, 100 - w - b), black: b };
}

/** 1.2k, 3.4M: counts as Lichess shortens them. */
export function compact(n: number): string {
  if (n >= 1e6) return `${(n / 1e6).toFixed(n >= 1e7 ? 0 : 1)}M`;
  if (n >= 1e4) return `${Math.round(n / 1e3)}k`;
  if (n >= 1e3) return `${(n / 1e3).toFixed(1)}k`;
  return String(n);
}

export function useExplorer(fen: MaybeRefOrGetter<string>, filters: MaybeRefOrGetter<ExplorerFilters>, wait = 200) {
  const lookup = useLookup<ExplorerAnswer>(() => explorerRequest(toValue(fen), toValue(filters)), wait);
  const answer = computed(() => lookup.data.value);
  return { ...lookup, answer };
}
