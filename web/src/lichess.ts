import { ref } from "vue";
import type { components } from "./api/schema";
import type { MatchSummary } from "./board/matchText";

export type LichessStatus = components["schemas"]["LichessStatus"];
export type LichessSettings = components["schemas"]["Settings"];
export type Speed = components["schemas"]["Speed"];

type Gameish = Pick<MatchSummary, "white" | "black" | "lichess" | "result" | "status">;

/** A clock as Lichess writes it, in minutes: `3+2`, `½+0`, `1.5+1`. */
export function clockLabel(seconds: number, increment: number): string {
  const minutes = { 15: "¼", 30: "½", 45: "¾" }[seconds] ?? String(Math.round((seconds / 60) * 100) / 100);
  return `${minutes}+${increment}`;
}

/** Lichess's speed for a clock: by the time a game is expected to take (initial + 40 increments). */
export function speedOf(seconds: number, increment: number): Speed {
  const total = seconds + 40 * increment;
  return total < 180 ? "bullet" : total < 480 ? "blitz" : total < 1500 ? "rapid" : "classical";
}

export const speedName = (s: string) => s.charAt(0).toUpperCase() + s.slice(1);

/** The clock of a game for people: Lichess games in minutes, local matches as the runner counts them. */
export function gameClock(m: Pick<MatchSummary, "time" | "lichess">): string {
  const t = m.time;
  if (t.type !== "clock") return "";
  return m.lichess ? clockLabel(t.seconds, t.increment) : `${t.seconds}+${t.increment}`;
}

/** The other side's player in a Lichess game, and the colour we played. */
export function opponentOf(m: Gameish) {
  const ours = m.lichess?.our_side ?? "white";
  return { side: ours, player: ours === "white" ? m.black : m.white };
}

export type Ours = "live" | "won" | "lost" | "drew" | "stopped";

/** How a game went for our bot. */
export function oursOf(m: Gameish): Ours {
  if (m.status === "running") return "live";
  if (m.result === "1/2-1/2") return "drew";
  if (m.result === "*") return "stopped";
  return (m.result === "1-0") === ((m.lichess?.our_side ?? "white") === "white") ? "won" : "lost";
}

export const oursText: Record<Ours, string> = {
  live: "Live",
  won: "Won",
  lost: "Lost",
  drew: "Drew",
  stopped: "Stopped",
};

/** Why a challenge was turned down, in words. Ours for an incoming challenge, the opponent's for one we sent. */
export function declineText(reason: string | null | undefined, incoming: boolean): string {
  switch (reason) {
    case "variant":
      return "not standard chess";
    case "timeControl":
      return "clock not accepted";
    case "tooFast":
      return "too fast";
    case "tooSlow":
      return "too slow";
    case "casual":
      return "rated games only";
    case "rated":
      return "casual games only";
    case "noBot":
      return "bots not accepted";
    case "onlyBot":
      return "bots only";
    case "later":
      return "busy with another game";
    case "generic":
      return incoming ? "no engine chosen" : "no reason given";
    default:
      return reason ?? "";
  }
}

type ChallengeUser = { name: string; title?: string | null; rating?: number | null };

export type ChallengeEvent = {
  id: string;
  direction: "incoming" | "outgoing";
  from?: ChallengeUser | null;
  to?: ChallengeUser | null;
  rated: boolean;
  speed: string;
  variant: string;
  clock?: { initial: number; increment: number } | null;
  decision: "accepted" | "declined" | "canceled";
  reason?: string | null;
};

/** What challenges did this session: the newest first. Kept here so it survives leaving the page. */
export const challenges = ref<ChallengeEvent[]>([]);

export function recordChallenge(data: unknown) {
  const c = data as ChallengeEvent;
  challenges.value = [c, ...challenges.value.filter((x) => x.id !== c.id)].slice(0, 20);
}

/** The other player of a challenge: who sent it, or who we sent it to. */
export function challengeWho(c: ChallengeEvent): ChallengeUser | undefined {
  return (c.direction === "incoming" ? c.from : c.to) ?? undefined;
}
