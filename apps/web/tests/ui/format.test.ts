import { describe, expect, it } from "vitest";
import {
  ago,
  figure,
  formatDate,
  formatDateTime,
  formatDuration,
  formatElo,
  formatEloPm,
  formatNumber,
  formatPercent,
  formatRating,
  formatRelative,
  formatResult,
  formatScore,
  NONE,
  resultOf,
  scoreSide,
  shortDate,
} from "../../src/format";

describe("numbers", () => {
  it("groups thousands, keeps decimals, and writes a true minus", () => {
    expect(formatNumber(3708)).toBe("3,708");
    expect(formatNumber(81.34, 1)).toBe("81.3");
    expect(formatNumber(1234567.5, 1)).toBe("1,234,567.5");
    expect(formatNumber(-12)).toBe("−12");
    expect(formatNumber(0)).toBe("0");
  });

  it("never writes a negative zero", () => {
    expect(formatNumber(-0.04, 1)).toBe("0.0");
    expect(formatNumber(-0)).toBe("0");
  });

  it("writes a missing figure as an en dash, never 0 or ?", () => {
    for (const none of [null, undefined, Number.NaN, Number.POSITIVE_INFINITY]) expect(formatNumber(none)).toBe(NONE);
    expect(NONE).toBe("–");
    expect(figure(null)).toBe(NONE);
  });
});

describe("Elo", () => {
  it("always shows a sign unless it is zero", () => {
    expect(formatElo(12)).toBe("+12");
    expect(formatElo(-5)).toBe("−5");
    expect(formatElo(0)).toBe("0");
    expect(formatElo(0.04, 1)).toBe("0.0");
    expect(formatElo(3.14159, 1)).toBe("+3.1");
    expect(formatElo(null)).toBe(NONE);
  });

  it("writes an estimate with its error", () => {
    expect(formatEloPm(3.14, 2.04)).toBe("+3.1 ± 2.0");
    expect(formatEloPm(-0.7, 5.9)).toBe("−0.7 ± 5.9");
    expect(formatEloPm(1, 0.5, 0)).toBe("+1 ± 1");
    expect(formatEloPm(null, 1)).toBe(NONE);
    expect(formatEloPm(1, undefined)).toBe(NONE);
  });
});

describe("percentages and ratings", () => {
  it("writes a percentage out of 100 with its sign", () => {
    expect(formatPercent(81.34)).toBe("81.3%");
    expect(formatPercent(100, 0)).toBe("100%");
    expect(formatPercent(null)).toBe(NONE);
  });

  it("marks a rating with ? only while it is provisional", () => {
    expect(formatRating(1840)).toBe("1,840");
    expect(formatRating(1345, true)).toBe("1,345?");
    expect(formatRating(1839.6)).toBe("1,840");
    expect(formatRating(null)).toBe(NONE);
    expect(formatRating("")).toBe(NONE);
    expect(formatRating("2000+")).toBe("2000+");
  });
});

describe("durations", () => {
  it("reads in the largest sensible units, whole numbers", () => {
    expect(formatDuration(0)).toBe("0 s");
    expect(formatDuration(45)).toBe("45 s");
    expect(formatDuration(59.6)).toBe("1 min");
    expect(formatDuration(200)).toBe("3 min");
    expect(formatDuration(3600)).toBe("1 h");
    expect(formatDuration(4800)).toBe("1 h 20 min");
    expect(formatDuration(86_400)).toBe("1 d");
    expect(formatDuration(190_000)).toBe("2 d 4 h");
    expect(formatDuration(null)).toBe(NONE);
    expect(formatDuration(-5)).toBe("0 s");
  });
});

describe("relative times and dates", () => {
  const now = Date.parse("2026-10-29T12:00:00Z");
  const before = (seconds: number) => new Date(now - seconds * 1000).toISOString();

  it("says how long ago, or how far ahead", () => {
    expect(formatRelative(before(10), now)).toBe("just now");
    expect(formatRelative(before(300), now)).toBe("5 minutes ago");
    expect(formatRelative(before(86_400), now)).toBe("yesterday");
    expect(formatRelative(before(-7200), now)).toBe("in 2 hours");
    expect(formatRelative(now - 300_000, now)).toBe("5 minutes ago");
  });

  it("keeps the older name working", () => {
    expect(ago(before(300), now)).toBe(formatRelative(before(300), now));
  });

  it("writes a day with the year only when it is not this one", () => {
    const here = new Date(2026, 9, 29);
    expect(formatDate(new Date(2026, 9, 12), here)).toBe("12 Oct");
    expect(formatDate(new Date(2025, 2, 2), here)).toBe("2 Mar 2025");
    expect(shortDate(new Date(2026, 9, 12).toISOString(), here)).toBe("12 Oct");
  });

  it("writes a moment in full for a tooltip", () => {
    expect(formatDateTime(new Date(2026, 9, 12, 14, 30))).toBe("12 Oct 2026, 14:30");
    expect(formatDateTime(new Date(2026, 0, 3, 9, 5))).toBe("3 Jan 2026, 09:05");
  });
});

describe("scores", () => {
  it("shows a sign and one decimal, mates as #n", () => {
    expect(formatScore({ cp: 62 })).toBe("+0.6");
    expect(formatScore({ cp: -423 })).toBe("−4.2");
    expect(formatScore({ cp: 0 })).toBe("0.0");
    expect(formatScore({ cp: -3 })).toBe("0.0");
    expect(formatScore(1.44)).toBe("+1.4");
    expect(formatScore({ mate: 3 })).toBe("#3");
    expect(formatScore({ mate: -2 })).toBe("#−2");
    expect(formatScore({ mate: 0 })).toBe("#0");
    expect(formatScore(null)).toBe(NONE);
  });

  it("says which side is ahead: none when level", () => {
    expect(scoreSide({ cp: 62 })).toBe("white");
    expect(scoreSide({ cp: -423 })).toBe("black");
    expect(scoreSide({ cp: 3 })).toBeNull();
    expect(scoreSide(0)).toBeNull();
    expect(scoreSide(-1.5)).toBe("black");
    expect(scoreSide({ mate: 4 })).toBe("white");
    expect(scoreSide({ mate: -1 })).toBe("black");
    expect(scoreSide(null)).toBeNull();
  });

  it("takes a mate already given as against the side to move", () => {
    expect(scoreSide({ mate: 0 }, "white")).toBe("black");
    expect(scoreSide({ mate: 0 }, "black")).toBe("white");
  });
});

describe("results", () => {
  it("records an outcome as the game record writes it", () => {
    expect(resultOf("white_wins")).toBe("1-0");
    expect(resultOf("black_wins")).toBe("0-1");
    expect(resultOf("draw")).toBe("1/2-1/2");
    expect(resultOf(null)).toBeNull();
    expect(resultOf(undefined)).toBeNull();
  });

  it("has no tone for a game you were not in", () => {
    expect(formatResult("1-0")).toEqual({ text: "1-0", tone: null });
    expect(formatResult("1/2-1/2")).toEqual({ text: "½-½", tone: null });
    expect(formatResult("*", "white")).toEqual({ text: "*", tone: null });
    expect(formatResult(null, "white")).toEqual({ text: "*", tone: null });
  });

  it("takes your side: 1-0 is a win for White and a loss for Black", () => {
    expect(formatResult("1-0", "white").tone).toBe("win");
    expect(formatResult("1-0", "black").tone).toBe("loss");
    expect(formatResult("0-1", "black").tone).toBe("win");
    expect(formatResult("0-1", "white").tone).toBe("loss");
    expect(formatResult("1/2-1/2", "black").tone).toBe("draw");
  });
});
