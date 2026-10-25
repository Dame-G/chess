import type { components } from "../api/schema";

type Game = components["schemas"]["GameSummary"];

/** How a game went for the new version: won, drew, lost, or nothing yet. */
export type Outcome = "W" | "D" | "L" | "";

export function outcome(g: Pick<Game, "white" | "result">): Outcome {
  if (g.result === "1/2-1/2") return "D";
  if (g.result !== "1-0" && g.result !== "0-1") return "";
  return (g.result === "1-0") === (g.white === "dev") ? "W" : "L";
}
