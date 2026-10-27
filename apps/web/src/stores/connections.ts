import { computed, ref } from "vue";
import { defineStore } from "pinia";
import { client, ok } from "../api/client";
import type {
  BotSettings,
  Challenge,
  ChallengeBody,
  ChallengeEvent,
  ConnectionKey,
  ConnectionStatus,
  Ratings,
  SeekMode,
  SeekRequestBody,
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
    const c = await ok(client.GET("/api/connections"));
    you.value = c.you;
    bot.value = c.bot;
    loaded.value = true;
  }

  const status = (key: ConnectionKey) => (key === "you" ? you : bot);
  const take = (c: ConnectionStatus) => {
    status(c.key).value = c;
  };

  /** Looks for a game on Lichess as You (rapid or slower: the Board API's rule). */
  async function seek(body: SeekRequestBody) {
    take(await ok(client.POST("/api/connections/you/seek", { body })));
  }
  /** Stops looking for a game as You. */
  async function cancelSeek() {
    take(await ok(client.DELETE("/api/connections/you/seek")));
  }
  /** Challenges someone, Stockfish (`ai_level`) or your bot (`my_bot`), as You or as the bot. */
  const challenge = (key: ConnectionKey, body: ChallengeBody) =>
    ok(client.POST("/api/connections/{key}/challenge", { params: { path: { key } }, body }));

  /** Answers a challenge to You (the bot answers its own by its settings). */
  const accept = (key: ConnectionKey, id: string) =>
    ok(client.POST("/api/connections/{key}/challenges/{id}/accept", { params: { path: { key, id } } })).then(take);
  const decline = (key: ConnectionKey, id: string, reason?: string) =>
    ok(
      client.POST("/api/connections/{key}/challenges/{id}/decline", {
        params: { path: { key, id } },
        body: { reason: reason ?? null },
      }),
    ).then(take);
  const cancel = (key: ConnectionKey, id: string) =>
    ok(client.DELETE("/api/connections/{key}/challenges/{id}", { params: { path: { key, id } } })).then(take);

  /**
   * Starts logging in to Lichess as You: the address to send the browser to. Lichess comes back to the server, which
   * sends the browser on to `page` of this app (the server wants a whole address: this page's origin, which is the
   * server itself or the hosted app).
   */
  const login = (page = "/settings") =>
    ok(client.POST("/api/connections/you/login", { body: { return_to: `${location.origin}${page}` } }));
  /** Stores a personal token for a connection; Lichess checks it. */
  async function setToken(key: ConnectionKey, token: string) {
    take(await ok(client.PUT("/api/connections/{key}/token", { params: { path: { key } }, body: { token } })));
  }
  /** Logs out (You: the token is revoked at Lichess) or disconnects the bot. */
  async function disconnect(key: ConnectionKey) {
    take(await ok(client.DELETE("/api/connections/{key}", { params: { path: { key } } })));
  }

  async function saveBotSettings(settings: BotSettings) {
    await ok(client.PUT("/api/connections/bot/settings", { body: settings }));
    await load();
  }
  async function setBotOnline(online: boolean) {
    take(await ok(client.POST("/api/connections/bot/online", { body: { online } })));
  }
  async function setBotSeek(mode: SeekMode) {
    take(await ok(client.POST("/api/connections/bot/seek", { body: { mode } })));
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
    cancelSeek,
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
