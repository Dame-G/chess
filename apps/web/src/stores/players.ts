import { ref } from "vue";
import { defineStore } from "pinia";
import { api } from "../api/client";
import type { Comparison, ImportRequest, Player, Profile, ProfileFilter, Speed, TopPlayer } from "../api/types";
import { addressQuery, type CompareAddress } from "../sheets/compare";
import { listen, reloadOnReconnect } from "./listen";

/** The filter a profile is read with when the sheet asks for none: every game, the newest 200. */
export const ALL_GAMES: ProfileFilter = { speeds: [], rated: null, since: null, limit: 200 };

const query = (f: ProfileFilter) => ({
  speeds: f.speeds.join(","),
  rated: f.rated === null ? undefined : String(f.rated),
  since: f.since ?? undefined,
  limit: f.limit,
});

/** The cache key of a profile: the player and the filter that was asked. */
export const profileKey = (key: string, f: ProfileFilter = ALL_GAMES) =>
  `${key}?${new URLSearchParams(Object.entries(query(f)).flatMap(([k, v]) => (v === undefined || v === "" ? [] : [[k, String(v)]])))}`;

const path = (key: string) => `/api/players/${encodeURIComponent(key)}`;

/**
 * Players: profiles (yours, Dameless's, anyone's on Lichess), the comparison of several, Lichess's leaderboard, and the
 * lookups that check a name. A profile or comparison that is on screen is read again when a review lands or an import
 * ends, so "Review them" shows its result without a reload.
 */
export const usePlayers = defineStore("players", () => {
  const profiles = ref<Record<string, Profile>>({});
  const comparison = ref<Comparison | null>(null);
  const top = ref<Partial<Record<Speed, TopPlayer[]>>>({});
  /** What is on screen: profiles and the comparison to keep current. */
  const watching = new Map<string, ProfileFilter>();
  let watchingCompare: CompareAddress | null = null;
  let timer: ReturnType<typeof setTimeout> | undefined;

  const profileOf = (key: string, filter: ProfileFilter = ALL_GAMES): Profile | undefined =>
    profiles.value[profileKey(key, filter)];

  async function loadProfile(key: string, filter: ProfileFilter = ALL_GAMES): Promise<Profile> {
    watching.set(key, filter);
    const p = await api.get<Profile>(`${path(key)}/profile`, query(filter));
    profiles.value[profileKey(key, filter)] = p;
    return p;
  }

  async function loadComparison(address: CompareAddress): Promise<Comparison> {
    watchingCompare = address;
    const q = addressQuery(address);
    const c = await api.get<Comparison>("/api/compare", { ...q, limit: address.limit });
    if (watchingCompare === address) comparison.value = c;
    return c;
  }

  /** Leaves the screen a profile or comparison was on. */
  function unwatch(key?: string) {
    if (key) watching.delete(key);
    else watchingCompare = null;
  }

  async function loadTop(speed: Speed): Promise<TopPlayer[]> {
    const list = await api.get<TopPlayer[]>("/api/players/top", { speed, nb: 10 });
    top.value[speed] = list;
    return list;
  }

  /** The Lichess player with this name; throws a 404 `ApiError` when Lichess has none. */
  const lookup = (name: string) => api.get<Player>(path(`lichess:${name.toLowerCase()}`));

  const importHistory = (key: string, request: ImportRequest = {}) => api.post(`${path(key)}/import`, request);

  /** Reads what is on screen again, soon: a review or an import just ended. */
  function refresh() {
    clearTimeout(timer);
    timer = setTimeout(() => {
      for (const [key, filter] of watching) void loadProfile(key, filter).catch(() => {});
      if (watchingCompare) void loadComparison(watchingCompare).catch(() => {});
    }, 600);
  }

  listen("review.upserted", refresh);
  listen("work.upserted", (w) => {
    if ((w.kind === "import" || w.kind === "review") && w.status === "done") refresh();
  });
  reloadOnReconnect(async () => refresh());

  return { profiles, comparison, top, profileOf, loadProfile, loadComparison, unwatch, loadTop, lookup, importHistory };
});
