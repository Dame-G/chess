/** Stamp tones for job, experiment and test states. */
export const statusTone = {
  queued: "neutral",
  running: "info",
  done: "success",
  passed: "success",
  failed: "danger",
  cancelled: "neutral",
} as const;

/** `+1.23` or `-0.50`. */
export function signed(n: number, digits = 2): string {
  return `${n > 0 ? "+" : ""}${n.toFixed(digits)}`;
}

/** A time control for people: `8+0.08` or `25k nodes`. */
export function timeControl(tc: { clock?: { seconds: number; increment: number }; nodes?: { nodes: number } }): string {
  if (tc.clock) return `${tc.clock.seconds}+${tc.clock.increment}`;
  if (tc.nodes) return `${tc.nodes.nodes / 1000}k nodes`;
  return "";
}
