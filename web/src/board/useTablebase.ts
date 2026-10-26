import { computed, toValue, type MaybeRefOrGetter } from "vue";
import { standardUci, useLookup } from "./useExplorer";

export type Category =
  | "win"
  | "unknown"
  | "syzygy-win"
  | "maybe-win"
  | "cursed-win"
  | "draw"
  | "blessed-loss"
  | "maybe-loss"
  | "syzygy-loss"
  | "loss";

interface TablebaseMove {
  uci: string;
  san: string;
  category: Category;
  dtz?: number | null;
  dtm?: number | null;
  zeroing?: boolean;
  checkmate?: boolean;
  stalemate?: boolean;
}
export interface TablebaseAnswer {
  category: Category;
  dtz?: number | null;
  dtm?: number | null;
  checkmate?: boolean;
  stalemate?: boolean;
  insufficient_material?: boolean;
  moves: TablebaseMove[];
}

/** Pieces on the board, from the placement field of a FEN. */
export function pieceCount(fen: string): number {
  return (fen.split(" ")[0] ?? "").replace(/[^a-zA-Z]/g, "").length;
}

/** The most pieces the tablebase holds. */
export const MAX_PIECES = 7;

const flip: Record<Category, Category> = {
  win: "loss",
  loss: "win",
  "syzygy-win": "syzygy-loss",
  "syzygy-loss": "syzygy-win",
  "maybe-win": "maybe-loss",
  "maybe-loss": "maybe-win",
  "cursed-win": "blessed-loss",
  "blessed-loss": "cursed-win",
  draw: "draw",
  unknown: "unknown",
};

/** Best first, for the side that moves. */
const rank: Record<Category, number> = {
  win: 0,
  "syzygy-win": 1,
  "maybe-win": 2,
  "cursed-win": 3,
  draw: 4,
  unknown: 5,
  "blessed-loss": 6,
  "maybe-loss": 7,
  "syzygy-loss": 8,
  loss: 9,
};

export interface TablebaseRow {
  uci: string;
  san: string;
  /** What the move earns the mover. Lichess gives it for the side that moves next, so it is turned round here. */
  outcome: Category;
  dtz?: number;
  dtm?: number;
  zeroing: boolean;
}

/** Moves with their outcome for the mover, best first: the fastest win, then the slowest loss. */
export function rowsOf(fen: string, answer: TablebaseAnswer): TablebaseRow[] {
  const rows = answer.moves.map<TablebaseRow>((m) => ({
    uci: standardUci(fen, m.uci),
    san: m.san,
    outcome: flip[m.category] ?? "unknown",
    dtz: m.dtz == null ? undefined : Math.abs(m.dtz),
    dtm: m.dtm == null ? undefined : Math.abs(m.dtm),
    zeroing: !!m.zeroing,
  }));
  const speed = (r: TablebaseRow) => r.dtm ?? r.dtz ?? 0;
  return rows.sort((a, b) => {
    if (rank[a.outcome] !== rank[b.outcome]) return rank[a.outcome] - rank[b.outcome];
    const losing = rank[a.outcome] > rank.draw;
    return losing ? speed(b) - speed(a) : speed(a) - speed(b);
  });
}

export function useTablebase(fen: MaybeRefOrGetter<string>, wait = 200) {
  const lookup = useLookup<TablebaseAnswer>(() => {
    const f = toValue(fen);
    return pieceCount(f) <= MAX_PIECES ? { path: "/api/lichess/tablebase", query: { fen: f } } : undefined;
  }, wait);
  const rows = computed(() => {
    const a = lookup.data.value;
    return a ? rowsOf(toValue(fen), a) : [];
  });
  return { ...lookup, rows };
}
