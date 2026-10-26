import type { components } from "../api/schema";

export type Match = components["schemas"]["Match"];
export type MatchSummary = components["schemas"]["MatchSummary"];
export type MatchTime = components["schemas"]["MatchTime"];
export type Side = "white" | "black";

/** A clock as a player's strip shows it: `4:07`, and tenths under ten seconds (`0:07.4`). */
export function clockText(ms: number): string {
  const total = Math.max(0, ms);
  if (total < 10_000) {
    const tenths = Math.floor(total / 100);
    return `0:0${Math.floor(tenths / 10)}.${tenths % 10}`;
  }
  const seconds = Math.floor(total / 1000);
  const h = Math.floor(seconds / 3600);
  const m = Math.floor((seconds % 3600) / 60);
  const s = seconds % 60;
  return h ? `${h}:${String(m).padStart(2, "0")}:${String(s).padStart(2, "0")}` : `${m}:${String(s).padStart(2, "0")}`;
}

/** The time control for people: `10+0.1`, `25k nodes`, `0.5 s a move`. */
export function matchTimeText(t: MatchTime): string {
  if (t.type === "clock") return `${t.seconds}+${t.increment}`;
  if (t.type === "nodes") return `${t.nodes >= 1000 ? `${t.nodes / 1000}k` : t.nodes} nodes`;
  return `${t.ms / 1000} s a move`;
}

type Ended = Pick<Match, "result" | "reason" | "status"> & { lichess?: Match["lichess"] };

/**
 * Who won and why, in plain words: "Black won on time", "Draw by threefold repetition". With the players' names the
 * winner reads "Black (main) won", so a match of two builds with the same name is still clear.
 */
export function outcomeText(m: Ended, names?: { white: string; black: string }): string {
  const side = m.result === "1-0" ? "White" : m.result === "0-1" ? "Black" : undefined;
  const winner = side && names ? `${side} (${names[side === "White" ? "white" : "black"]})` : side;
  const loser = side === "White" ? "Black" : "White";
  switch (m.reason) {
    case "aborted":
      return m.lichess ? "Aborted" : "Stopped before the end";
    case "resigned":
      return `${loser} resigned`;
    case "agreement":
      return "Draw agreed";
    case "abandoned":
      return winner ? `${loser} left the game` : "Game abandoned";
    case "no_start":
      return "Aborted, the game never started";
    case "cheat":
      return winner ? `${winner} won, ${loser} was disqualified` : "Game ended by a fair-play ruling";
    case "variant_end":
      return winner ? `${winner} won by the variant's rule` : "Draw by the variant's rule";
    case "checkmate":
      return `${winner} won by checkmate`;
    case "time":
      return winner ? `${winner} won on time` : "Draw: a clock ran out, but the other side could not mate";
    case "resignation":
      return `${winner} won, ${loser} was losing by too much (adjudicated)`;
    case "crash":
      return `${winner} won, ${loser}'s engine stopped`;
    case "illegal_move":
      return `${winner} won, ${loser}'s engine played an illegal move`;
    case "stalemate":
      return "Draw by stalemate";
    case "repetition":
      return "Draw by threefold repetition";
    case "fifty_moves":
      return "Draw by the fifty-move rule";
    case "insufficient_material":
      return "Draw by insufficient material";
    case "draw_adjudication":
      return "Draw, level for many moves (adjudicated)";
    default:
      return m.status === "running" ? "In progress" : winner ? `${winner} won` : "Draw";
  }
}

/** How a result reads for a side: `1`, `0`, `½`. */
export function sideScore(result: Match["result"], side: Side): string {
  if (result === "1/2-1/2") return "½";
  if (result === "*") return "";
  return (result === "1-0") === (side === "white") ? "1" : "0";
}

/** The clock after the move at `ply` (0 is before any move), for a side, or `null` when the match has no clock. */
export function clockAt(m: Match, ply: number, side: Side): number | null {
  const start = m.time.type === "clock" ? m.time.seconds * 1000 : null;
  if (start === null) return null;
  // The clock of a side only changes on its own moves: the latest of its moves at or before `ply`.
  for (let i = Math.min(ply, m.moves.length) - 1; i >= 0; i--) {
    const mv = m.moves[i]!;
    if (mv.side === side) return side === "white" ? (mv.clock_white_ms ?? start) : (mv.clock_black_ms ?? start);
  }
  return start;
}

/** The side a person plays in a game against a build, if one does. */
export function youSide(m: Pick<Match, "white" | "black">): Side | undefined {
  return m.white.human ? "white" : m.black.human ? "black" : undefined;
}

/** How a game you played came out, in the second person: "You won on time", "You resigned", "Draw agreed". */
export function youOutcome(m: Ended & Pick<Match, "white" | "black">): string {
  const me = youSide(m);
  const won = (m.result === "1-0" && me === "white") || (m.result === "0-1" && me === "black");
  if (m.status === "aborted" || m.reason === "aborted")
    return m.status === "running" ? "In progress" : "Game cancelled";
  switch (m.reason) {
    case "resigned":
      return won ? "Your opponent resigned" : "You resigned";
    case "checkmate":
      return won ? "You won by checkmate" : "You were checkmated";
    case "time":
      return m.result === "1/2-1/2" ? outcomeText(m) : won ? "You won on time" : "You lost on time";
    case "crash":
    case "illegal_move":
      return won ? "You won, the engine stopped" : "You lost, the engine stopped";
    default:
      return outcomeText(m);
  }
}

/** A game list row's result for a game you played: `Won`, `Lost`, `Draw`. */
export function youResult(m: Pick<MatchSummary, "result" | "white" | "black">): string {
  const me = youSide(m);
  if (m.result === "1/2-1/2") return "Draw";
  if (m.result === "*") return "·";
  return (m.result === "1-0") === (me === "white") ? "Won" : "Lost";
}
