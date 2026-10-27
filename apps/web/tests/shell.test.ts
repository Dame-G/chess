import { describe, expect, it } from "vitest";
import { agoShort } from "../src/rail/rail";

describe("the rail's short times", () => {
  const now = Date.parse("2026-01-15T12:00:00Z");
  const before = (ms: number) => new Date(now - ms).toISOString();
  const minute = 60_000;

  it("reads minutes, hours, days, weeks and years", () => {
    expect(agoShort(before(10_000), now)).toBe("now");
    expect(agoShort(before(5 * minute), now)).toBe("5m");
    expect(agoShort(before(3 * 60 * minute), now)).toBe("3h");
    expect(agoShort(before(2 * 24 * 60 * minute), now)).toBe("2d");
    expect(agoShort(before(21 * 24 * 60 * minute), now)).toBe("3w");
    expect(agoShort(before(400 * 24 * 60 * minute), now)).toBe("1y");
  });

  it("treats a start in the future as now", () => {
    expect(agoShort(before(-5 * minute), now)).toBe("now");
  });
});
