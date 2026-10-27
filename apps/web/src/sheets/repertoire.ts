import type { Departure, Repertoire, RepertoireNode, Side } from "../api/types";

/** A position as the repertoire keys it: the FEN without the move counters. */
export const positionKey = (fen: string): string => fen.split(" ").slice(0, 4).join(" ");

/** What your own choices show: your top moves at a position. What you met shows more, since there is more to learn. */
const YOURS = 2;
const REPLIES = 3;

/** A row of the compact tree: one move of the repertoire, with how often, how much and how it went. */
export interface OutlineRow {
  /** The moves from the start that lead here, which makes a row's identity (transpositions have several). */
  id: string;
  depth: number;
  /** The position after this move. */
  key: string;
  ply: number;
  /** Whether You made the move. */
  yours: boolean;
  uci: string;
  san: string;
  /** `1.` or `1…`, for display. */
  number: string;
  games: number;
  /** Share of the games that reached the position before it. */
  share: number;
  /** Your points per game, 0 to 1. */
  score: number;
  /** The move takes the game out of theory. */
  leaves: boolean;
  /** Moves lead on from it. */
  expandable: boolean;
  /** Every move from the start, to open the line on the board. */
  line: string[];
}

const byKey = (rep: Repertoire): Map<string, RepertoireNode> => new Map(rep.nodes.map((n) => [n.key, n]));

/** The moves shown at a node: your top choices, or the top replies you met. */
function shownMoves(n: RepertoireNode) {
  return n.moves.slice(0, n.your_turn ? YOURS : REPLIES);
}

/** The root of the tree: the start position, the node reached in no moves. */
const rootOf = (rep: Repertoire): RepertoireNode | undefined => rep.nodes.find((n) => n.ply === 0) ?? rep.nodes[0];

/**
 * The tree as an outline for a compact list: depth first, each move a row, a node's moves shown only where its row is
 * expanded (`expanded` holds row ids). A position reached again along the same path (a cycle) ends the branch.
 */
export function outline(rep: Repertoire, expanded: ReadonlySet<string>): OutlineRow[] {
  const nodes = byKey(rep);
  const root = rootOf(rep);
  const rows: OutlineRow[] = [];
  if (!root) return rows;

  const walk = (node: RepertoireNode, path: string[], seen: ReadonlySet<string>, depth: number) => {
    for (const m of shownMoves(node)) {
      const line = [...path, m.uci];
      const id = line.join(" ");
      const next = nodes.get(m.next);
      const onward = next && !seen.has(next.key) && shownMoves(next).length > 0;
      const ply = path.length + 1;
      rows.push({
        id,
        depth,
        key: m.next,
        ply,
        yours: node.your_turn,
        uci: m.uci,
        san: m.san,
        number: `${Math.ceil(ply / 2)}${ply % 2 === 1 ? "." : "…"}`,
        games: m.games,
        share: m.share,
        score: m.score,
        leaves: m.leaves_theory === true,
        expandable: !!onward,
        line,
      });
      if (onward && expanded.has(id)) walk(next, line, new Set([...seen, next.key]), depth + 1);
    }
  };
  walk(root, [], new Set([root.key]), 0);
  return rows;
}

/** The ids of the rows along the main line (the most played move at each position), to open the tree on. */
export function mainLineIds(rep: Repertoire, depth = 6): Set<string> {
  const nodes = byKey(rep);
  const ids = new Set<string>();
  let node = rootOf(rep);
  const path: string[] = [];
  const seen = new Set<string>();
  while (node && path.length < depth && !seen.has(node.key)) {
    seen.add(node.key);
    const m = node.moves[0];
    if (!m) break;
    path.push(m.uci);
    ids.add(path.join(" "));
    node = nodes.get(m.next);
  }
  return ids;
}

/** Where games left theory: your own departures and your opponents', the most common first, a few of each. */
export function departures(rep: Repertoire, limit = 5): { yours: Departure[]; theirs: Departure[] } {
  const sorted = [...rep.departures].sort((a, b) => b.games - a.games || a.ply - b.ply);
  return {
    yours: sorted.filter((d) => d.by_you).slice(0, limit),
    theirs: sorted.filter((d) => !d.by_you).slice(0, limit),
  };
}

/** `62%`. */
export const share = (x: number): string => `${Math.round(x * 100)}%`;

/** The quiet line Explore shows when its position is in your repertoire, or `null`. */
export function repertoireNote(reps: Partial<Record<Side, Repertoire | null>>, fen: string): string | null {
  const key = positionKey(fen);
  for (const rep of Object.values(reps)) {
    const node = rep?.nodes.find((n) => n.key === key);
    const top = node?.moves[0];
    if (!node || !top) continue;
    const figures = `${share(top.share)}, ${share(top.score)} score`;
    return node.your_turn
      ? `Your repertoire: you play ${top.san} here (${figures})`
      : `Your repertoire: your opponents play ${top.san} here (${figures})`;
  }
  return null;
}
