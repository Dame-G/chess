/** Plain words for the jargon the sheets cannot avoid. A `Term` shows one in a toggletip. */
export const GLOSSARY = {
  elo: {
    term: "Elo",
    text: "A rating difference. +10 Elo means winning about 54 points in 100 against the same opponent. The ± is the 95 % margin: the true value is very likely inside it.",
  },
  nelo: {
    term: "nElo",
    text: "Elo scaled by how often games are drawn, so results at short and long time controls can be compared. Tests set their goals in nElo.",
  },
  sprt: {
    term: "SPRT",
    text: "A test that plays games until the evidence is strong enough either way, instead of a fixed number. It stops early when a change is clearly good or clearly not.",
  },
  llr: {
    term: "LLR",
    text: "The running tally of evidence in a test. It moves up when the change looks better and down when it does not. Reaching the top line means kept; the bottom line means dropped.",
  },
  bench: {
    term: "Bench",
    text: "The number of positions the engine searches in a fixed test run. If a change leaves it the same, the engine plays exactly as before. It is the engine's fingerprint.",
  },
  accuracy: {
    term: "Accuracy",
    text: "How close your moves were to the engine's best, as Lichess measures it: 100 is perfect play. It counts big slips more than small ones.",
  },
  acpl: {
    term: "Centipawn loss",
    text: "The average number of hundredths of a pawn each move gave away compared with the best move. Lower is better.",
  },
  performance: {
    term: "Performance",
    text: "The rating that would match these results: the opponents' average rating, plus or minus for how many more games were won than lost.",
  },
  pairs: {
    term: "Pairs",
    text: "Tests play each opening twice with the colours swapped, so the engines get the same chances. One such pair is the unit of evidence.",
  },
  scaling: {
    term: "Clock scaling",
    text: "Time controls are stretched to this machine's speed, so 8+0.08 means what it means on the reference machine. The test records the factor it used.",
  },
  screen: {
    term: "Quick screen",
    text: "A fast first test at a very short time control that only throws out changes that are clearly worse. Passing it is not evidence of a gain.",
  },
  tune: {
    term: "Tune",
    text: "Many short games move the engine's numbers a little at a time towards values that win more. The result must still pass a normal test before it is kept.",
  },
  champion: {
    term: "Champion",
    text: "The current best version of Dameless: the tip of the fork's main line. New ideas are tested against it.",
  },
  calibration: {
    term: "Calibration",
    text: "Dameless's levels play each other, and you, in the background. The results fix what each level is really worth, instead of trusting the first guess of its rating.",
  },
  net: {
    term: "Network",
    text: "The part of the engine that judges a position, learned from millions of positions. A new network can make the engine stronger without changing a line of its code.",
  },
  superbatch: {
    term: "Superbatch",
    text: "A fixed number of batches of positions. Training is counted in superbatches, and a checkpoint is saved every few of them.",
  },
  loss: {
    term: "Loss",
    text: "How far the network's judgements are from the answers in the data. It should fall as training goes on; lower is better, and only games show whether a net is stronger.",
  },
} as const;

export type TermKey = keyof typeof GLOSSARY;
