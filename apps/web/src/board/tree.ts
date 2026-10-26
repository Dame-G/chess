import { makeFen } from "chessops/fen";
import { playUci, setup, type Line } from "./position";

/** One position of the tree, reached by `san` from its parent. The root has no move. */
export interface TreeNode {
  id: number;
  /** Moves from the root: the root is 0. */
  ply: number;
  uci: string;
  san: string;
  fen: string;
  /** The first child is the main continuation, the rest are variations. */
  children: TreeNode[];
  parent: TreeNode | null;
}

/**
 * The tree of moves Explore works in: the game's moves as the main line, and whatever is played on the board from any
 * position as variations. A cursor is a node; stepping forward follows the first child, back the parent.
 */
export class GameTree {
  readonly root: TreeNode;
  private next = 1;

  constructor(line: Line) {
    const pos = setup(line.fen);
    if (!pos) throw new Error("Not a legal position");
    this.root = { id: 0, ply: 0, uci: "", san: "", fen: makeFen(pos.toSetup()), children: [], parent: null };
    let at = this.root;
    for (const uci of line.moves) {
      const child = this.play(at, uci);
      if (!child) break;
      at = child;
    }
  }

  /** Plays a move from a node: the child that already has it, else a new variation (the main line if it is the first). */
  play(from: TreeNode, uci: string): TreeNode | undefined {
    const pos = setup(from.fen);
    if (!pos) return undefined;
    const played = playUci(pos, uci);
    if (!played) return undefined;
    const existing = from.children.find((c) => c.uci === played.uci);
    if (existing) return existing;
    const node: TreeNode = {
      id: this.next++,
      ply: from.ply + 1,
      uci: played.uci,
      san: played.san,
      fen: makeFen(pos.toSetup()),
      children: [],
      parent: from,
    };
    from.children.push(node);
    return node;
  }

  /** The nodes from the root's first move to `node`. */
  path(node: TreeNode): TreeNode[] {
    const out: TreeNode[] = [];
    for (let n: TreeNode | null = node; n && n.parent; n = n.parent) out.push(n);
    return out.reverse();
  }

  /** The line to `node`, as the server takes it. */
  line(node: TreeNode): Line {
    return { fen: this.root.fen, moves: this.path(node).map((n) => n.uci) };
  }

  /** The end of the line through `node`: its first children as far as they go. */
  end(node: TreeNode): TreeNode {
    let n = node;
    while (n.children[0]) n = n.children[0];
    return n;
  }

  /** Whether `node` is on the main line (every step the first child). */
  isMain(node: TreeNode): boolean {
    for (let n: TreeNode | null = node; n && n.parent; n = n.parent) if (n.parent.children[0] !== n) return false;
    return true;
  }

  /** The node at `ply` on the main line, or its last. */
  mainAt(ply: number): TreeNode {
    let n = this.root;
    while (n.ply < ply && n.children[0]) n = n.children[0];
    return n;
  }

  /** Makes the variation holding `node` the main continuation at each fork above it. */
  promote(node: TreeNode) {
    for (let n: TreeNode | null = node; n && n.parent; n = n.parent) {
      const siblings = n.parent.children;
      const i = siblings.indexOf(n);
      if (i > 0) siblings.unshift(...siblings.splice(i, 1));
    }
  }

  /** Removes `node` and everything after it. Returns where the cursor should go: its parent. */
  remove(node: TreeNode): TreeNode {
    const parent = node.parent ?? this.root;
    if (node.parent) node.parent.children.splice(node.parent.children.indexOf(node), 1);
    return parent;
  }

  /** Finds a node by id. */
  find(id: number, from: TreeNode = this.root): TreeNode | undefined {
    if (from.id === id) return from;
    for (const c of from.children) {
      const hit = this.find(id, c);
      if (hit) return hit;
    }
    return undefined;
  }
}

/** The move number of the move that leads to `node`, from the start's full-move counter and side to move. */
function numberOf(root: TreeNode, node: TreeNode): { number: number; white: boolean } {
  const parts = root.fen.split(" ");
  const blackFirst = parts[1] === "b";
  const first = Number(parts[5] ?? 1);
  const index = node.ply - 1 + (blackFirst ? 1 : 0);
  return { number: first + Math.floor(index / 2), white: index % 2 === 0 };
}

/** A move in a variation: its node and its text, numbered where a reader needs the number. */
interface Token {
  node: TreeNode;
  text: string;
  /** How deep the variation is: 1 for one off the main line. */
  depth: number;
}

/** A row of the move list: a numbered pair of main-line moves, or the variations that branch at a main-line move. */
export type Row =
  | { type: "moves"; number: number; white: TreeNode | null; black: TreeNode | null }
  | { type: "variations"; key: string; lines: Token[][] };

/** One variation as tokens, its own variations inlined after the move they branch from. */
function variation(root: TreeNode, start: TreeNode, depth: number): Token[] {
  const out: Token[] = [];
  let n: TreeNode | undefined = start;
  let first = true;
  while (n) {
    const { number, white } = numberOf(root, n);
    const text = white ? `${number}. ${n.san}` : first ? `${number}… ${n.san}` : n.san;
    out.push({ node: n, text, depth });
    first = false;
    const parent: TreeNode = n.parent!;
    for (const alt of parent.children.slice(1))
      if (parent.children[0] === n) out.push(...variation(root, alt, depth + 1));
    // After a nested variation, the next move takes its number again so it reads on its own.
    const nextNode: TreeNode | undefined = n.children[0];
    if (nextNode && parent.children[0] === n && parent.children.length > 1) first = true;
    n = nextNode;
  }
  return out;
}

/**
 * The move list's rows: the main line in numbered pairs, and after each main-line move that has alternatives a row
 * of them, each with its own variations inline. The pair breaks around a variations row, as Lichess's list does.
 */
export function rows(tree: GameTree): Row[] {
  const out: Row[] = [];
  let pending: { number: number; white: TreeNode | null } | null = null;
  const flush = () => {
    if (pending) out.push({ type: "moves", number: pending.number, white: pending.white, black: null });
    pending = null;
  };
  let n = tree.root.children[0];
  while (n) {
    const { number, white } = numberOf(tree.root, n);
    if (white) {
      flush();
      pending = { number, white: n };
    } else {
      out.push({ type: "moves", number, white: pending?.white ?? null, black: n });
      pending = null;
    }
    const alts = n.parent!.children.slice(1);
    if (alts.length) {
      flush();
      out.push({ type: "variations", key: `v${n.id}`, lines: alts.map((a) => variation(tree.root, a, 1)) });
    }
    n = n.children[0];
  }
  flush();
  return out;
}
