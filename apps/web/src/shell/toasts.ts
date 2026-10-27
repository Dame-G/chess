import { h, onScopeDispose, watch } from "vue";
import { useRoute, useRouter } from "vue-router";
import { useToast } from "@eos-ui/vue";
import { EButton } from "@eos-ui/vue/components/EButton";
import { onEvent } from "../api/stream";
import type { GameId, GameSummary } from "../api/types";
import { useBoard } from "../board/board";
import { isLive } from "../board/modes";
import { useConnections } from "../stores/connections";
import { useGames } from "../stores/games";

/**
 * The toasts the app raises by itself, from the stream: a game of yours starting elsewhere (Open), an incoming
 * challenge to You (Accept · Decline; the bot answers its own by its rules) and an idea finishing. Call once, in the app's root.
 */
export function useEventToasts() {
  const toast = useToast();
  const router = useRouter();
  const board = useBoard();
  const games = useGames();
  const connections = useConnections();
  const announced = new Set<GameId>();
  const challengeToasts = new Map<string, string>();

  const button = (label: string, onClick: () => void, variant: "outline" | "secondary" = "outline") =>
    h(EButton, { size: "sm", variant, onClick }, () => label);

  onScopeDispose(
    onEvent("game.upserted", (g) => {
      if (!g.you || !isLive(g) || announced.has(g.id)) return;
      announced.add(g.id);
      // A moment for a game this tab started to arrive on the board first.
      setTimeout(() => announce(g), 600);
    }),
  );
  function announce(g: GameSummary) {
    if (board.gameId === g.id || games.startedHere.has(g.id)) return;
    const opponent = g.you === "white" ? g.black : g.white;
    const id = toast.show({
      key: `game-${g.id}`,
      title: `Your game against ${opponent.name} started`,
      tone: "info",
      action: () =>
        button("Open", () => {
          toast.dismiss(id);
          void router.push({ path: "/", query: { game: String(g.id) } });
        }),
    });
  }
  onScopeDispose(
    onEvent("challenge", (e) => {
      const c = e.challenge;
      if (e.event !== "created" || c.direction !== "incoming" || c.connection !== "you") {
        const shown = challengeToasts.get(c.id);
        if (e.event !== "created" && shown) toast.dismiss(shown);
        return;
      }
      const rating = c.opponent.rating ? ` (${c.opponent.rating})` : "";
      const id = toast.show({
        key: `challenge-${c.id}`,
        title: `${c.opponent.name}${rating} challenges ${c.connection === "you" ? "you" : "Dameless"}`,
        message: `${c.rated ? "Rated" : "Casual"} ${c.speed.replace("_", " ")}`,
        tone: "info",
        duration: 0,
        action: () =>
          h("span", { class: "toast-actions" }, [
            button("Accept", () => void connections.accept(c.connection, c.id).catch(() => {}), "secondary"),
            button("Decline", () => void connections.decline(c.connection, c.id).catch(() => {})),
          ]),
      });
      challengeToasts.set(c.id, id);
    }),
  );
  onScopeDispose(
    onEvent("idea.upserted", (i) => {
      if (i.status !== "kept" && i.status !== "dropped") return;
      toast.show({
        key: `idea-${i.id}-${i.status}`,
        title: i.status === "kept" ? `Kept: ${i.title}` : `Dropped: ${i.title}`,
        message: i.verdict ?? undefined,
        tone: i.status === "kept" ? "success" : "neutral",
        action: () => button("Open", () => void router.push(`/dameless/ideas/${i.id}`)),
      });
    }),
  );
}

/**
 * What Lichess said when a login came back (`?lichess=connected` or `?lichess=error&message=…` on the page the login
 * started from), as a toast; the query is then taken off the address. Call once, in the app's root.
 */
export function useLoginResult() {
  const toast = useToast();
  const route = useRoute();
  const router = useRouter();
  watch(
    () => route.query["lichess"],
    (result) => {
      if (result !== "connected" && result !== "error") return;
      const message = typeof route.query["message"] === "string" ? route.query["message"] : undefined;
      if (result === "connected") toast.show({ title: "Your Lichess account is linked", tone: "success" });
      else
        toast.show({
          title: "Lichess login did not work",
          message: message ?? "Lichess did not give the app access. Try logging in again.",
          tone: "warning",
        });
      const query = { ...route.query };
      delete query["lichess"];
      delete query["message"];
      void router.replace({ query });
    },
    { immediate: true },
  );
}
