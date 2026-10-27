import { computed, ref, shallowRef, watch } from "vue";
import { defineStore } from "pinia";
import type { Color } from "@lichess-org/chessground/types";
import type { GameId, Score, Side } from "../api/types";
import { pawns, scoreText } from "../format";
import { ApiError } from "../api/client";
import { useDrills } from "../stores/drills";
import { useEvaluation } from "../stores/evaluation";
import { useGames } from "../stores/games";
import { usePolicy } from "../stores/policy";
import { stored } from "../stored";
import { isLive, modesFor, type Mode } from "./modes";
import { INITIAL_FEN, playUci, setup, view, type Position } from "./position";
import { GameTree, type TreeNode } from "./tree";

/** One position of a game: the FEN after `ply` moves and the move that led there. */
export interface Ply {
  fen: string;
  uci?: string;
  san?: string;
  turn: Side;
}

/** The board's state: what is on it (a game or a free position), the move shown, the mode, which way up. */
export const useBoard = defineStore("board", () => {
  const games = useGames();
  const policy = usePolicy();
  const evaluation = useEvaluation();
  const drills = useDrills();

  /** The game on the board, or `null` for a free position (Explore). */
  const gameId = ref<GameId | null>(null);
  /** The move shown in a game: 0 is the start. */
  const ply = ref(0);
  /** Whether the board keeps up with a live game's newest move. */
  const following = ref(true);
  /** Explore's tree, and the node on the board. The tree grows in place; `treeVersion` counts its changes. */
  const tree = shallowRef<GameTree>(new GameTree({ fen: INITIAL_FEN, moves: [] }));
  const node = shallowRef<TreeNode>(tree.value.root);
  const treeVersion = ref(0);
  const chosenMode = ref<Mode | null>(null);
  /** The game the address named that does not exist, so the panel can say so instead of showing an empty board. */
  const missing = ref<GameId | null>(null);
  const flipped = ref(false);
  /** Explore's engine, on or off (Space). */
  const engineOn = stored("engine", true);

  const game = computed(() => (gameId.value !== null && games.open?.summary.id === gameId.value ? games.open : null));
  const summary = computed(() => (gameId.value === null ? null : (games.summaries[gameId.value] ?? null)));

  /** Every position of the open game, from the start. */
  const plies = computed<Ply[]>(() => {
    const g = game.value;
    const pos = setup(g?.start_fen ?? INITIAL_FEN);
    if (!pos) return [];
    const out: Ply[] = [{ fen: g?.start_fen ?? INITIAL_FEN, turn: pos.turn }];
    for (const m of g?.moves ?? []) {
      const played = playUci(pos, m.uci);
      if (!played) break;
      out.push({ fen: view(pos).fen, uci: played.uci, san: played.san, turn: pos.turn });
    }
    return out;
  });
  const lastPly = computed(() => Math.max(0, plies.value.length - 1));

  /** A drill is on the board: it takes the whole board and the panel, and the game or position under it waits. */
  const drilling = computed(() => drills.drilling);
  const modes = computed<Mode[]>(() => (drilling.value ? ["drill"] : modesFor(summary.value)));
  const mode = computed<Mode>(() =>
    chosenMode.value && modes.value.includes(chosenMode.value) ? chosenMode.value : modes.value[0]!,
  );
  const exploring = computed(() => mode.value === "explore");

  /** The board's position: Explore's node, else the game at the move shown. */
  const position = computed<Position>(() => {
    if (drilling.value && drills.active) {
      const pos = setup(drills.active.fen);
      if (pos) return view(pos);
    }
    if (exploring.value) {
      const pos = setup(node.value.fen)!;
      return view(pos, node.value.uci || undefined);
    }
    const p = plies.value[Math.min(ply.value, lastPly.value)];
    const pos = setup(p?.fen ?? INITIAL_FEN)!;
    return view(pos, p?.uci);
  });

  /** You at the bottom in your games, the bot's side in its own; flipping turns that round. */
  const orientation = computed<Color>(() => {
    if (drilling.value && drills.active) return drills.active.side;
    const s = summary.value;
    let base: Side = "white";
    if (s?.you) base = s.you;
    else if (s?.source.type === "lichess" && s.source.connection === "bot")
      base = s.white.player === "dameless" ? "white" : "black";
    return flipped.value ? (base === "white" ? "black" : "white") : base;
  });

  /** The query that names what is on the board now: `game` and `ply` (none while following a live game), or `fen`. */
  const address = computed((): Record<string, string> => {
    if (drilling.value && drills.active) return { drill: String(drills.active.id) };
    if (gameId.value !== null) {
      const live = summary.value ? isLive(summary.value) : false;
      return live && following.value
        ? { game: String(gameId.value) }
        : { game: String(gameId.value), ply: String(ply.value) };
    }
    const fen = node.value.fen;
    return fen === INITIAL_FEN ? {} : { fen };
  });

  /** Opens a game on the board at a move (its end when left out, and following it while it is live). */
  async function openGame(id: GameId, at?: number) {
    drills.leave();
    if (gameId.value !== id) chosenMode.value = null;
    gameId.value = id;
    missing.value = null;
    following.value = at === undefined;
    try {
      await games.openGame(id);
    } catch (e) {
      if (e instanceof ApiError && e.status === 404) missing.value = id;
      throw e;
    }
    ply.value = at === undefined ? lastPly.value : Math.min(at, lastPly.value);
  }

  /** Puts a free position on the board, in Explore. */
  function openFree(fen = INITIAL_FEN, moves: string[] = []) {
    drills.leave();
    gameId.value = null;
    missing.value = null;
    games.closeGame();
    chosenMode.value = null;
    tree.value = new GameTree({ fen, moves });
    node.value = tree.value.end(tree.value.root);
  }

  /** Steps to a move: a ply of the game, or along Explore's line. */
  function go(target: number) {
    if (drilling.value) return;
    if (exploring.value) {
      const path = tree.value.path(tree.value.end(node.value));
      const here = node.value.ply;
      const want = Math.max(0, Math.min(target, tree.value.end(node.value).ply));
      if (want === here) return;
      // Within the current line: back along the path to the node, forward along its first children.
      node.value = want === 0 ? tree.value.root : (path.find((n) => n.ply === want) ?? node.value);
      return;
    }
    ply.value = Math.max(0, Math.min(target, lastPly.value));
    following.value = ply.value === lastPly.value;
  }

  const cursor = computed(() => (exploring.value ? node.value.ply : ply.value));
  const atEnd = computed(() => (exploring.value ? !node.value.children.length : ply.value >= lastPly.value));

  /** A move made on the board in Explore: into the tree (an existing child or a new variation). */
  function exploreMove(uci: string) {
    const next = tree.value.play(node.value, uci);
    if (!next) return;
    node.value = next;
    treeVersion.value++;
  }

  function selectNode(n: TreeNode) {
    node.value = n;
  }

  /** Changes the panel's tab. Into Explore from a game, the tree starts from the game with the board's move. */
  function setMode(next: Mode) {
    if (next === "explore" && !exploring.value && game.value) {
      tree.value = new GameTree({
        fen: game.value.start_fen ?? INITIAL_FEN,
        moves: game.value.moves.map((m) => m.uci),
      });
      node.value = tree.value.mainAt(ply.value);
    }
    if (next !== "explore" && exploring.value && gameId.value !== null && tree.value.isMain(node.value)) {
      ply.value = Math.min(node.value.ply, lastPly.value);
    }
    chosenMode.value = next;
  }

  function flip() {
    flipped.value = !flipped.value;
  }

  // A live game's new move moves the board with it while it follows.
  watch(lastPly, (n) => {
    if (following.value || ply.value > n) ply.value = n;
  });

  /** The score to show for the position on the board, White POV, with where it came from. */
  const shownScore = computed<{ score: Score; turn: Side; depth?: number } | null>(() => {
    if (!policy.allowed || mode.value === "play" || drilling.value) return null;
    const turn = position.value.turn;
    if (exploring.value) {
      const info = evaluation.info;
      const t = evaluation.target;
      if (!info || !t || t.fen !== tree.value.root.fen || t.moves.join() !== tree.value.line(node.value).moves.join())
        return null;
      const best = info.lines[0];
      return best ? { score: best.score, turn, depth: info.depth } : null;
    }
    const review = games.review;
    if (review?.plies.length) {
      const p = ply.value === 0 ? review.plies[0] : review.plies[ply.value - 1];
      if (p) return { score: ply.value === 0 ? p.eval_before : p.eval_after, turn, depth: p.depth };
    }
    const t = games.thinking;
    if (mode.value === "watch" && atEnd.value && t?.score) return { score: t.score, turn, depth: t.depth };
    const moves = game.value?.moves ?? [];
    for (let i = Math.min(ply.value, moves.length) - 1; i >= 0; i--) {
      const m = moves[i]!;
      if (m.eval) return { score: m.eval, turn, depth: m.depth ?? undefined };
    }
    return null;
  });
  const shownEval = computed(() => {
    const s = shownScore.value;
    return s ? { pawns: pawns(s.score, s.turn), text: scoreText(s.score, s.turn) } : undefined;
  });

  // Explore's engine follows the board while it is on and analysis is allowed.
  watch(
    () => [exploring.value && engineOn.value && policy.allowed, node.value, tree.value] as const,
    ([on]) => {
      if (!on) {
        void evaluation.stop();
        return;
      }
      const line = tree.value.line(node.value);
      void evaluation.start({ fen: line.fen, moves: line.moves });
    },
    // Also at once: a page opened straight onto a sheet (`/dameless`) explores the start position and never moves it.
    { immediate: true },
  );

  return {
    gameId,
    ply,
    following,
    tree,
    node,
    treeVersion,
    flipped,
    engineOn,
    game,
    summary,
    plies,
    lastPly,
    modes,
    mode,
    drilling,
    exploring,
    position,
    orientation,
    cursor,
    atEnd,
    shownScore,
    shownEval,
    address,
    missing,
    openGame,
    openFree,
    go,
    exploreMove,
    selectNode,
    setMode,
    flip,
  };
});
