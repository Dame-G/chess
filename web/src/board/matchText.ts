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

type Ended = Pick<Match, "result" | "reason" | "status">;

/** Who won and why, in plain words: "Black won on time", "Draw by threefold repetition". */
export function outcomeText(m: Ended): string {
  const winner = m.result === "1-0" ? "White" : m.result === "0-1" ? "Black" : undefined;
  const loser = winner === "White" ? "Black" : "White";
  switch (m.reason) {
    case "aborted":
      return "Stopped before the end";
    case "checkmate":
      return `${winner} won by checkmate`;
    case "time":
      return `${winner} won on time`;
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
