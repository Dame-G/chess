import { makeFen } from "chessops/fen";
import { parsePgn, startingPosition } from "chessops/pgn";
import { parseSan } from "chessops/san";
import { uciOf } from "./position";

interface Ply {
  san: string;
  uci: string;
  /** The position after the move. */
  fen: string;
  /** The mover's evaluation from White's side, in pawns; mates are ±100. */
  eval: number | null;
  depth: number | null;
  nodes: number | null;
  /** Seconds the move took. */
  time: number | null;
}

export interface Game {
  headers: Record<string, string>;
  startFen: string;
  plies: Ply[];
}

/** Reads fastchess's move comment: `+1.06/9 0.063s, n=9351, sd=14, nps=149921` (or `+M5/...` for a mate). */
export function readComment(comment: string): Pick<Ply, "eval" | "depth" | "nodes" | "time"> {
  const m = /^([+-]?)(M?)(\d+(?:\.\d+)?)\/(\d+)\s+([\d.]+)s/.exec(comment.trim());
  const nodes = /n=(\d+)/.exec(comment);
  if (!m) return { eval: null, depth: null, nodes: nodes ? Number(nodes[1]) : null, time: null };
  const sign = m[1] === "-" ? -1 : 1;
  const value = m[2] ? 100 : Number(m[3]);
  return { eval: sign * value, depth: Number(m[4]), nodes: nodes ? Number(nodes[1]) : null, time: Number(m[5]) };
}

export function readGame(pgn: string): Game | undefined {
  const game = parsePgn(pgn)[0];
  if (!game) return undefined;
  const start = startingPosition(game.headers);
  if (start.isErr) return undefined;
  const pos = start.value;
  const startFen = makeFen(pos.toSetup());
  const plies: Ply[] = [];
  for (const node of game.moves.mainline()) {
    const move = parseSan(pos, node.san);
    if (!move) break;
    const mover = pos.turn;
    const uci = uciOf(pos, move);
    pos.play(move);
    const info = readComment(node.comments?.join(" ") ?? "");
    // Engines report their own side's score; the graph reads from White's.
    if (info.eval !== null && mover === "black") info.eval = -info.eval;
    plies.push({ san: node.san, uci, fen: makeFen(pos.toSetup()), ...info });
  }
  return { headers: Object.fromEntries(game.headers), startFen, plies };
}
