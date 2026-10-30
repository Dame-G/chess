import { computed, ref } from "vue";
import { defineStore } from "pinia";
import { client, ok } from "../api/client";
import type { Challenge, ChallengeBody, ConnectionStatus, Ratings, SeekRequestBody } from "../api/types";
import { listen, reloadOnReconnect } from "./listen";

const OFF: ConnectionStatus = {
  key: "you",
  configured: false,
  account: null,
  state: "off",
  error: null,
  playing: [],
  challenges: [],
  you: null,
};

/** The rating people know a Lichess account by: blitz, else rapid, else bullet, else classical. */
function headlineRating(ratings: Ratings | undefined): number | null {
  for (const speed of ["blitz", "rapid", "bullet", "classical"] as const) {
    const r = ratings?.[speed];
    if (r) return r.rating;
  }
  return null;
}

/** Your Lichess connection and the challenges waiting on it. */
export const useConnections = defineStore("connections", () => {
  const you = ref<ConnectionStatus>({ ...OFF });
  const loaded = ref(false);

  /** Challenges waiting on your account. */
  const challenges = computed<Challenge[]>(() => you.value.challenges);
  const youRating = computed(() => headlineRating(you.value.account?.ratings));

  async function load() {
    const c = await ok(client.GET("/api/connections"));
    you.value = c.you;
    loaded.value = true;
  }

  /** Takes a connection's new state: yours; any other connection the server may name is not the app's concern. */
  const take = (c: ConnectionStatus) => {
    if (c.key === "you") you.value = c;
  };

  /** Looks for a game on Lichess (rapid or slower: the Board API's rule). */
  async function seek(body: SeekRequestBody) {
    take(await ok(client.POST("/api/connections/you/seek", { body })));
  }
  /** Stops looking for a game. */
  async function cancelSeek() {
    take(await ok(client.DELETE("/api/connections/you/seek")));
  }
  /** Challenges someone, or Stockfish (`ai_level`). */
  const challenge = (body: ChallengeBody) =>
    ok(client.POST("/api/connections/{key}/challenge", { params: { path: { key: "you" } }, body }));

  /** Answers a challenge to you. */
  const accept = (id: string) =>
    ok(client.POST("/api/connections/{key}/challenges/{id}/accept", { params: { path: { key: "you", id } } })).then(
      take,
    );
  const decline = (id: string, reason?: string) =>
    ok(
      client.POST("/api/connections/{key}/challenges/{id}/decline", {
        params: { path: { key: "you", id } },
        body: { reason: reason ?? null },
      }),
    ).then(take);
  const cancel = (id: string) =>
    ok(client.DELETE("/api/connections/{key}/challenges/{id}", { params: { path: { key: "you", id } } })).then(take);

  /**
   * Starts logging in to Lichess: the address to send the browser to. Lichess comes back to the server, which sends
   * the browser on to `page` of this app (the server wants a whole address: this page's origin, which is the server
   * itself or the hosted app).
   */
  const login = (page = "/settings") =>
    ok(client.POST("/api/connections/you/login", { body: { return_to: `${location.origin}${page}` } }));
  /** Logs out: the token is revoked at Lichess and forgotten here. */
  async function logout() {
    take(await ok(client.DELETE("/api/connections/{key}", { params: { path: { key: "you" } } })));
  }

  listen("connection.upserted", take);
  listen("challenge", (e) => {
    if (e.challenge.connection !== "you") return;
    const rest = you.value.challenges.filter((x) => x.id !== e.challenge.id);
    you.value = { ...you.value, challenges: e.event === "created" ? [...rest, e.challenge] : rest };
  });
  reloadOnReconnect(load);

  return {
    you,
    challenges,
    youRating,
    loaded,
    load,
    seek,
    cancelSeek,
    challenge,
    accept,
    decline,
    cancel,
    login,
    logout,
  };
});
