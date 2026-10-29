import { ref } from "vue";
import { defineStore } from "pinia";
import type { GameId } from "../api/types";
import { useGames, type GamesQuery } from "../stores/games";
import { queryOf, type GameFilter } from "./rail";

/** How many games one request brings. */
export const PAGE = 50;

/** Where a list stands in the server's games: the last game it holds (the next page starts after it) and whether that is all. */
export interface Feed {
  /** The first page was asked for (answered or failed). */
  started: boolean;
  loading: boolean;
  /** The server has no more. */
  done: boolean;
  /** When the oldest game loaded so far started: the rail shows nothing older than this until another page comes. */
  floor: string | null;
  cursor: GameId | null;
  error: string | null;
}

const fresh = (): Feed => ({ started: false, loading: false, done: false, floor: null, cursor: null, error: null });
const NEW: Readonly<Feed> = Object.freeze(fresh());

/** A list's name: a rail filter, or one trial's games. */
export type FeedKey = GameFilter | `trial:${number}`;

const queryFor = (key: FeedKey): GamesQuery =>
  key.startsWith("trial:") ? { trial: Number(key.slice(6)) } : queryOf(key as GameFilter);

/**
 * What the Games rail has asked the server for. Each filter (and each open trial) is its own list, paged `PAGE` at a
 * time after its last game, so the filter you pick is answered by the server and never by what happens to be held:
 * a long history is reached by scrolling, and a trial's thousands of games cannot crowd out the rest. The games
 * themselves are held in the games store; this keeps only where each list has got to.
 */
export const useRailFeed = defineStore("rail-feed", () => {
  const games = useGames();
  const feeds = ref<Record<string, Feed>>({});

  /** A list's state; one never asked for reads as new. Reading it changes nothing, so a computed may. */
  const of = (key: FeedKey): Feed => feeds.value[key] ?? NEW;

  /** The state a list changes, made on first use. */
  function entry(key: FeedKey): Feed {
    // Answer the stored object (the reactive one), not the fresh one the assignment would hand back.
    feeds.value[key] ??= fresh();
    return feeds.value[key]!;
  }

  /** Brings the next page of a list; does nothing while one is on its way or when there are no more. */
  async function more(key: FeedKey): Promise<void> {
    const feed = entry(key);
    if (feed.loading || feed.done) return;
    feed.loading = true;
    feed.error = null;
    try {
      const page = await games.page({
        ...queryFor(key),
        limit: PAGE,
        ...(feed.cursor !== null ? { before: feed.cursor } : {}),
      });
      const last = page.at(-1);
      if (last) {
        feed.cursor = last.id;
        feed.floor = last.started_at;
      }
      if (page.length < PAGE) {
        feed.done = true;
        feed.floor = null;
      }
    } catch (e) {
      feed.error = e instanceof Error ? e.message : String(e);
    } finally {
      feed.loading = false;
      feed.started = true;
    }
  }

  /** Asks again after a failure. */
  function retry(key: FeedKey): Promise<void> {
    entry(key).error = null;
    return more(key);
  }

  /** Starts a list that has not been asked for yet. */
  function start(key: FeedKey): Promise<void> {
    return of(key).started || of(key).loading ? Promise.resolve() : more(key);
  }

  return { of, more, retry, start };
});
