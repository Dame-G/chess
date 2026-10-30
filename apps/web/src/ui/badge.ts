import type { HealthLevel, WorkStatus } from "../api/types";

/** The tone of a badge: what it says about the thing, never a decoration. `act` is cognac, the live and the chosen. */
export type BadgeTone = "neutral" | "win" | "loss" | "warn" | "info" | "act";

/** A piece of work's status. */
export const workTone: Record<WorkStatus, BadgeTone> = {
  queued: "neutral",
  running: "info",
  paused: "warn",
  done: "win",
  failed: "loss",
  stopped: "neutral",
};

/** A health check's level: the tone, and the word that says it (a healthy check shows no badge at all). */
export const healthTone: Record<HealthLevel, { tone: BadgeTone; word: string }> = {
  ok: { tone: "win", word: "Ready" },
  info: { tone: "info", word: "Note" },
  warn: { tone: "warn", word: "Check" },
  fail: { tone: "loss", word: "Missing" },
};
