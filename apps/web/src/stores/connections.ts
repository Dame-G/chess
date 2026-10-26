import { computed, ref } from "vue";
import { defineStore } from "pinia";
import { api } from "../api/client";
import type {
  BotSettings,
  Challenge,
  ChallengeEvent,
  Color,
  ConnectionKey,
  Connections,
  ConnectionStatus,
  Ratings,
  SeekMode,
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
  challenges: [],
  you: null,
  bot: null,
});

/** The rating people know a Lichess account by: blitz, else rapid, else bullet, else classical. */
function headlineRating(ratings: Ratings | undefined): number | null {
  for (const speed of ["blitz", "rapid", "bullet", "classical"] as const) {
    const r = ratings?.[speed];
    if (r) return r.rating;
  }
  return null;
}

/** Challenges answered lately that the bot's card remembers. */
const RECENT = 8;

/** The two Lichess connections, You and the bot, and the challenges waiting on either. */
export const useConnections = defineStore("connections", () => {
  const you = ref<ConnectionStatus>(off("you"));
  const bot = ref<ConnectionStatus>(off("bot"));
  /** Challenges that were answered, newest first, from the stream: a short memory, not history. */
  const recent = ref<ChallengeEvent[]>([]);
  const loaded = ref(false);

  /** Challenges waiting on either account. */
  const challenges = computed<Challenge[]>(() => [...you.value.challenges, ...bot.value.challenges]);
  const youRating = computed(() => headlineRating(you.value.account?.ratings));
  const botRating = computed(() => headlineRating(bot.value.account?.ratings));

  async function load() {
    const c = await api.get<Connections>("/api/connections");
    you.value = c.you;
    bot.value = c.bot;
    loaded.value = true;
  }

  const status = (key: ConnectionKey) => (key === "you" ? you : bot);
  const take = (c: ConnectionStatus) => {
    status(c.key).value = c;
  };

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
  const cancel = (key: ConnectionKey, id: string) => api.delete(`/api/connections/${key}/challenges/${id}`);

  /** Starts logging in to Lichess as You: the address to send the browser to. */
  const login = () => api.post<{ url: string }>("/api/connections/you/login", { return_to: "/settings" });
  /** Stores a personal token for a connection; Lichess checks it. */
  async function setToken(key: ConnectionKey, token: string) {
    take(await api.put<ConnectionStatus>(`/api/connections/${key}/token`, { token }));
  }
  /** Logs out (You: the token is revoked at Lichess) or disconnects the bot. */
  async function disconnect(key: ConnectionKey) {
    take(await api.delete<ConnectionStatus>(`/api/connections/${key}`));
  }

  async function saveBotSettings(settings: BotSettings) {
    take(await api.put<ConnectionStatus>("/api/connections/bot/settings", settings));
  }
  async function setBotOnline(online: boolean) {
    take(await api.post<ConnectionStatus>("/api/connections/bot/online", { online }));
  }
  async function setBotSeek(mode: SeekMode) {
    take(await api.post<ConnectionStatus>("/api/connections/bot/seek", { mode }));
  }

  listen("connection.upserted", take);
  listen("challenge", (e) => {
    const c = status(e.challenge.connection);
    const rest = c.value.challenges.filter((x) => x.id !== e.challenge.id);
    c.value = { ...c.value, challenges: e.event === "created" ? [...rest, e.challenge] : rest };
    if (e.event !== "created")
      recent.value = [e, ...recent.value.filter((x) => x.challenge.id !== e.challenge.id)].slice(0, RECENT);
  });
  reloadOnReconnect(load);

  return {
    you,
    bot,
    challenges,
    recent,
    youRating,
    botRating,
    loaded,
    load,
    seek,
    challenge,
    accept,
    decline,
    cancel,
    login,
    setToken,
    disconnect,
    saveBotSettings,
    setBotOnline,
    setBotSeek,
  };
});
