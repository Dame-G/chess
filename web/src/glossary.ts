/** The words of engine testing, explained for someone new to it. `Term` shows them beside the word. */
export const glossary = {
  elo: {
    term: "Elo",
    text: "A rating difference. +10 Elo means the changed version scores about 51.4% against main. Top engines improve a few Elo at a time.",
  },
  nelo: {
    term: "Normalized Elo",
    text: "Elo scaled by how decisive the games are, so results compare across time controls. The test's bounds are in it.",
  },
  sprt: {
    term: "SPRT",
    text: "The sequential test that decides when to stop: it keeps playing until the evidence is strong enough to call the change better or worse, so a clear result stops early.",
  },
  llr: {
    term: "Evidence (LLR)",
    text: "How strongly the games so far favour one answer. The test stops when it crosses the accept or the reject line.",
  },
  scaled: {
    term: "Scaled clock",
    text: "Games are played on a clock stretched to this machine's speed, as OpenBench does, so a result at 8+0.08 means the same here as on Reckless's own testers.",
  },
  bounds: {
    term: "Elo range tested",
    text: "The two answers the test chooses between, in normalized Elo: [0, 3] asks “is it at least 3 better, or no better at all?”",
  },
  los: {
    term: "Chance stronger",
    text: "The likelihood, from the games so far, that the changed version is truly stronger rather than lucky.",
  },
  pairs: {
    term: "Game pairs",
    text: "Each opening is played twice, the versions swapping colours, so a lopsided opening cannot decide the result. A pair scores 0 to 2 points.",
  },
  bench: {
    term: "Fingerprint (bench)",
    text: "The number of positions the engine searches on a fixed set of test positions. Any change to how it plays changes it; Reckless writes it in every commit.",
  },
  nps: {
    term: "Speed",
    text: "Positions searched per second (nodes per second). Faster search sees further in the same time.",
  },
  depth: {
    term: "Depth",
    text: "How many moves ahead the engine has searched in full. The second number is the deepest line it followed.",
  },
  eval: {
    term: "Evaluation",
    text: "Who is better and by how much, in pawns from White's side: +1.00 is about a pawn up for White. M3 is mate in 3, and −M3 is mate in 3 for Black.",
  },
  branch: {
    term: "Branch",
    text: "A named copy of the engine with one change. Each idea lives on its own branch; main is the baseline every change is tested against. Merge a branch into main if it passes.",
  },
  spsa: {
    term: "SPSA",
    text: "A way to tune many constants at once: each round tries two slightly different copies of the engine against each other and moves every constant a little towards the winner.",
  },
  lichessSpeed: {
    term: "Lichess speed",
    text: "Lichess calls a game by its expected length, the first clock plus 40 moves of increment: under 3 minutes is bullet, under 8 blitz, under 25 rapid, and longer classical. Apps may only start rapid or slower games from a seek, and blitz from a challenge.",
  },
  build: {
    term: "Build",
    text: "A compiled engine from one commit of a version (a branch). Each is built once and kept.",
  },
  accuracy: {
    term: "Accuracy",
    text: "Lichess's measure of how close a player's moves were to the engine's best, from 0 to 100. Each move scores by how many winning chances it gave away; the game's accuracy weighs the moves where the position was sharpest. Here an engine of ours searches every position.",
  },
  acpl: {
    term: "Average centipawn loss",
    text: "On average, how much worse than the engine's best move each move was, in hundredths of a pawn. Lower is better; 20 is strong club play, and a master is under 15.",
  },
  performance: {
    term: "Performance",
    text: "The rating that would have scored this result against these opponents: their average rating plus 400 × (wins − losses) ÷ games.",
  },
  brilliant: {
    term: "Brilliant !!",
    text: "The engine's best move, or nearly, that gives up at least two pawns of material along its best line, when you were not already winning and are fine afterwards. A real sacrifice that works.",
  },
  great: {
    term: "Great !",
    text: "The only move that holds: the engine's best, while every other move gives away at least 15 points of winning chance. Not a simple recapture.",
  },
  best: { term: "Best ★", text: "The move the engine would have played." },
  excellent: { term: "Excellent", text: "Within 2 points of winning chance of the best move." },
  good: { term: "Good", text: "Not the best, but not an inaccuracy either." },
  book: { term: "Book", text: "A move still inside the named opening." },
  inaccuracy: {
    term: "Inaccuracy ?!",
    text: "Lichess's judgement: the move gave up 5 points of winning chance or more.",
  },
  mistake: { term: "Mistake ?", text: "Lichess's judgement: the move gave up 10 points of winning chance or more." },
  blunder: { term: "Blunder ??", text: "Lichess's judgement: the move gave up 15 points of winning chance or more." },
  miss: {
    term: "Miss",
    text: "A mistake or blunder right after the opponent's own, when you were clearly better: the chance was let go, but the game was not lost.",
  },
  timeTrouble: {
    term: "Time-trouble blunders",
    text: "Blunders played with less than a tenth of the starting clock left.",
  },
} as const;

export type GlossaryKey = keyof typeof glossary;
