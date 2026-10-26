import { ref } from "vue";
import type { components } from "./api/schema";
import type { MatchSummary } from "./board/matchText";

export type LichessStatus = components["schemas"]["LichessStatus"];
export type LichessSettings = components["schemas"]["Settings"];
export type Speed = components["schemas"]["Speed"];
export type SeekState = components["schemas"]["SeekStatus"];
export type SeekMode = components["schemas"]["SeekMode"];
export type MeStatus = components["schemas"]["MeStatus"];
export type MeChallenge = components["schemas"]["ChallengeInfo"];
export type Ratings = components["schemas"]["Ratings"];

type Gameish = Pick<MatchSummary, "white" | "black" | "lichess" | "result" | "status">;

/** A clock as Lichess writes it, in minutes: `3+2`, `½+0`, `1.5+1`. */
export function clockLabel(seconds: number, increment: number): string {
  const minutes = { 15: "¼", 30: "½", 45: "¾" }[seconds] ?? String(Math.round((seconds / 60) * 100) / 100);
  return `${minutes}+${increment}`;
}

/** The clocks Lichess offers for a game against a bot: minutes plus seconds a move. */
const seekPresets = ["1+0", "2+1", "3+0", "3+2", "5+0", "5+3", "10+0", "10+5", "15+10"];

/** Seconds of a clock written `m+s` (`3+2` is 180 and 2); minutes may be fractions, as in `0.5+0`. */
export function parseClock(text: string): { initial: number; increment: number } {
  const [m, s] = text.split("+");
  return { initial: Math.round(Number(m) * 60), increment: Number(s ?? 0) };
}

/** The presets, with any clock set elsewhere (the command line) added, shortest first. */
export function clockOptions(chosen: readonly string[]): { value: string; label: string }[] {
  const length = (c: string) => {
    const { initial, increment } = parseClock(c);
    return initial + 40 * increment;
  };
  return [...new Set([...seekPresets, ...chosen])]
    .sort((a, b) => length(a) - length(b))
    .map((c) => ({ value: c, label: c }));
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
  /** The Lichess decline key: ours for an incoming challenge, theirs for one we sent. */
  reason?: string | null;
  /** A few words more, such as why the daily limit of bot games declined it. */
  detail?: string | null;
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

// ---- Your own account (Board API): what Lichess allows apps to start.

/** Lichess's estimate of a game's length: initial + 40 increments, in seconds. */
const estimated = (seconds: number, increment: number) => seconds + 40 * increment;

/** A seek must be rapid or slower (8 minutes estimated), a challenge may be blitz (3 minutes). */
const SEEK_MIN = 480;
const CHALLENGE_MIN = 180;

const seekClocks = ["10+0", "10+5", "15+10", "30+0", "30+20"];
const challengeClocks = ["3+0", "3+2", "5+0", "5+3", "10+0", "10+5", "15+10", "30+0", "30+20"];

/** The clocks to pick from for a seek or a challenge, shortest first, each allowed by the Board API. */
export function meClockOptions(kind: "seek" | "challenge"): { value: string; label: string }[] {
  return (kind === "seek" ? seekClocks : challengeClocks).map((c) => ({ value: c, label: c }));
}

/** Whether Lichess lets an app start this clock as a seek or a challenge. */
export function clockAllowed(kind: "seek" | "challenge", clock: string): boolean {
  const { initial, increment } = parseClock(clock);
  return estimated(initial, increment) >= (kind === "seek" ? SEEK_MIN : CHALLENGE_MIN);
}

export const tokenScopes = ["board:play", "challenge:read", "challenge:write"] as const;

/** Lichess's token form with the scopes Dameless needs already ticked. */
export const personalTokenUrl = `https://lichess.org/account/oauth/token/create?${tokenScopes
  .map((s) => `scopes[]=${s}`)
  .join("&")}&description=Dameless`;

/** The ratings a person has, in Lichess's order; speeds never played are left out. */
export function ratingList(ratings: Ratings | undefined): { speed: Speed; rating: number }[] {
  return (["bullet", "blitz", "rapid", "classical"] as const).flatMap((speed) => {
    const rating = ratings?.[speed];
    return rating ? [{ speed, rating }] : [];
  });
}

/** `5+3 rated`: how a challenge reads in a sentence. */
export function challengeLine(c: Pick<MeChallenge, "clock" | "rated" | "days">): string {
  const clock = c.clock ? clockLabel(c.clock.initial, c.clock.increment) : c.days ? `${c.days} days` : "";
  return [clock, c.rated ? "rated" : "casual"].filter(Boolean).join(" ");
}

/** The game you are playing now, for the status bar: here (a match) or on lichess.org (not playable here). */
export function liveGame(
  me: Pick<MeStatus, "playing" | "elsewhere"> | undefined,
  games: readonly (Gameish & { id: number })[] | undefined,
): { id?: number; url?: string; opponent?: string } | undefined {
  const id = me?.playing[0];
  if (id !== undefined) {
    const m = games?.find((g) => g.id === id);
    return { id, opponent: m ? opponentOf(m).player.name : undefined };
  }
  const away = me?.elsewhere[0];
  return away ? { url: away.url } : undefined;
}
