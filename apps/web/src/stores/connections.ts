import { computed, ref } from "vue";
import { defineStore } from "pinia";
import { api } from "../api/client";
import type {
  Challenge,
  Color,
  ConnectionKey,
  Connections,
  ConnectionStatus,
  Ratings,
  TimeControl,
} from "../api/types";
import { listen, reloadOnReconnect } from "./listen";

const off = (key: ConnectionStatus["key"]): ConnectionStatus => ({
  key,
  configured: false,
  account: null,
  state: "off",
  error: null,
  playing: [],
});

/** The rating people know a Lichess account by: blitz, else rapid, else bullet, else classical. */
function headlineRating(ratings: Ratings | undefined): number | null {
  for (const speed of ["blitz", "rapid", "bullet", "classical"] as const) {
    const r = ratings?.[speed];
    if (r) return r.rating;
  }
  return null;
}

/** The two Lichess connections, You and the bot, and the challenges waiting on either. */
export const useConnections = defineStore("connections", () => {
  const you = ref<ConnectionStatus>(off("you"));
  const bot = ref<ConnectionStatus>(off("bot"));
  const challenges = ref<Challenge[]>([]);
  const loaded = ref(false);

  const youRating = computed(() => headlineRating(you.value.account?.ratings));
  const botRating = computed(() => headlineRating(bot.value.account?.ratings));

  async function load() {
    const c = await api.get<Connections>("/api/connections");
    you.value = c.you;
    bot.value = c.bot;
    loaded.value = true;
  }

  /** Looks for a game on Lichess as You (rapid or slower: the Board API's rule). */
  const seek = (body: { time: TimeControl; rated: boolean; color: Color }) =>
    api.post("/api/connections/you/seek", body);
  /** Challenges someone, Stockfish (`ai_level`) or your bot (`my_bot`), as You or as the bot. */
  const challenge = (
    key: ConnectionKey,
    body: ({ username: string } | { ai_level: number } | { my_bot: true }) & {
      time: TimeControl;
      rated: boolean;
      color: Color;
    },
  ) => api.post<Challenge>(`/api/connections/${key}/challenge`, body);

  const accept = (key: ConnectionKey, id: string) => api.post(`/api/connections/${key}/challenges/${id}/accept`);
  const decline = (key: ConnectionKey, id: string) => api.post(`/api/connections/${key}/challenges/${id}/decline`);

  listen("connection.upserted", (c) => {
    if (c.key === "you") you.value = c;
    else bot.value = c;
  });
  listen("challenge", (c) => {
    const rest = challenges.value.filter((x) => x.id !== c.id);
    if (c.event === "created") {
      const { event, ...challenge } = c;
      void event;
      challenges.value = [...rest, challenge];
    } else challenges.value = rest;
  });
  reloadOnReconnect(load);

  return { you, bot, challenges, youRating, botRating, loaded, load, seek, challenge, accept, decline };
});
