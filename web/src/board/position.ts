import { Chess } from "chessops/chess";
import { chessgroundDests } from "chessops/compat";
import { INITIAL_FEN, makeFen, parseFen } from "chessops/fen";
import { makeSanVariation } from "chessops/san";
import { makeUci, parseUci } from "chessops/util";
import type { Color, Key } from "@lichess-org/chessground/types";

/** A position as the runner takes it: a starting FEN (or `startpos`) and the UCI moves played from it. */
export interface Line {
  fen: string;
  moves: string[];
}

export interface Position {
  /** The FEN after the moves. */
  fen: string;
  turn: Color;
  check: boolean;
  /** Legal moves for the board: from-square to destinations. */
  dests: Map<Key, Key[]>;
  lastMove?: [Key, Key];
  /** Moves that were not legal, with everything after them, are dropped. */
  moves: string[];
}

function setup(fen: string): Chess | undefined {
  const parsed = parseFen(fen === "startpos" ? INITIAL_FEN : fen);
  if (parsed.isErr) return undefined;
  const pos = Chess.fromSetup(parsed.value);
  return pos.isOk ? pos.value : undefined;
}

export function isValidFen(fen: string): boolean {
  return setup(fen) !== undefined;
}

/** Plays a line from its FEN. `undefined` when the FEN is not a legal position. */
export function play(line: Line): Position | undefined {
  const pos = setup(line.fen);
  if (!pos) return undefined;
  const played: string[] = [];
  let lastMove: [Key, Key] | undefined;
  for (const uci of line.moves) {
    const move = parseUci(uci);
    if (!move || !pos.isLegal(move)) break;
    pos.play(move);
    played.push(makeUci(move));
    lastMove = [uci.slice(0, 2) as Key, uci.slice(2, 4) as Key];
  }
  return {
    fen: makeFen(pos.toSetup()),
    turn: pos.turn,
    check: pos.isCheck(),
    dests: chessgroundDests(pos) as Map<Key, Key[]>,
    lastMove,
    moves: played,
  };
}

/** A UCI move from the board, promoting to a queen when a pawn reaches the last rank. */
export function boardMove(position: Position, from: Key, to: Key): string {
  const pawn = position.fen.split(" ")[0]!;
  const rank = to[1];
  const piece = pieceAt(pawn, from);
  const promotes = (piece === "P" && rank === "8") || (piece === "p" && rank === "1");
  return `${from}${to}${promotes ? "q" : ""}`;
}

function pieceAt(boardFen: string, square: Key): string | undefined {
  const file = square.charCodeAt(0) - 97;
  const rank = 8 - Number(square[1]);
  const row = boardFen.split("/")[rank] ?? "";
  let col = 0;
  for (const ch of row) {
    if (/\d/.test(ch)) col += Number(ch);
    else if (col++ === file) return ch;
  }
  return undefined;
}

/** A principal variation in SAN (`1. e4 e5 2. Nf3`), as far as its moves are legal. */
export function sanLine(fen: string, pv: string[]): string {
  const pos = setup(fen);
  if (!pos) return pv.join(" ");
  const moves = [];
  for (const uci of pv) {
    const move = parseUci(uci);
    if (!move) break;
    moves.push(move);
  }
  return makeSanVariation(pos, moves);
}

export type Score = { cp: number } | { mate: number };

/** A score for display from White's side: `+0.35`, `-1.20`, `#3`, `#-2`. Engines report it for the side to move. */
export function whiteScore(score: Score, turn: Color): string {
  const sign = turn === "white" ? 1 : -1;
  if ("mate" in score) return `#${score.mate * sign}`;
  const pawns = (score.cp * sign) / 100;
  return `${pawns > 0 ? "+" : ""}${pawns.toFixed(2)}`;
}
