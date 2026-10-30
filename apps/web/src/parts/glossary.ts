/** Plain words for the jargon the sheets cannot avoid. A `Term` shows one in a toggletip. */
export const GLOSSARY = {
  elo: {
    term: "Elo",
    text: "A rating difference. +10 Elo means winning about 54 points in 100 against the same opponent. The ± is the 95 % margin: the true value is very likely inside it.",
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
  calibration: {
    term: "Calibration",
    text: "Dameless's levels play each other, and you, in the background. The results fix what each level is really worth, instead of trusting the first guess of its rating.",
  },
} as const;

export type TermKey = keyof typeof GLOSSARY;
