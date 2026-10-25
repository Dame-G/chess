/** The words of engine testing, explained for someone new to it. `Term` shows them beside the word. */
export const glossary = {
  elo: {
    term: "Elo",
    text: "A rating difference. +10 Elo means the new version scores about 51.4% against the old one. Top engines improve a few Elo at a time.",
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
  bounds: {
    term: "Bounds",
    text: "The two answers the test chooses between, in normalized Elo: [0, 3] asks “is it at least 3 better, or no better at all?”",
  },
  los: {
    term: "Chance stronger",
    text: "The likelihood, from the games so far, that the new version is truly stronger rather than lucky.",
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
    text: "A named line of changes in the Reckless code. Each idea lives on its own branch; main is the current best.",
  },
  build: {
    term: "Engine",
    text: "A compiled version of Reckless from one commit of a branch. Each is built once and kept.",
  },
} as const;

export type GlossaryKey = keyof typeof glossary;
