import { describe, expect, it, vi } from "vitest";

vi.stubGlobal("location", { hostname: "127.0.0.1" });
const { initialConnection, DEFAULT_RUNNER } = await import("../src/api/runner");
const { parseEvents } = await import("../src/api/events");

describe("initialConnection", () => {
  it("uses the page's own origin when the runner serves it", () => {
    expect(initialConnection({ hostname: "127.0.0.1" }, null)).toEqual({ url: "", token: "" });
    expect(initialConnection({ hostname: "localhost" }, '{"url":"http://x","token":"t"}')).toEqual({
      url: "",
      token: "",
    });
  });

  it("remembers a runner on the hosted site, and defaults to the local one", () => {
    expect(initialConnection({ hostname: "chess.dame.gg" }, null)).toEqual({ url: DEFAULT_RUNNER, token: "" });
    expect(initialConnection({ hostname: "chess.dame.gg" }, '{"url":"https://box","token":"t"}')).toEqual({
      url: "https://box",
      token: "t",
    });
    expect(initialConnection({ hostname: "chess.dame.gg" }, "not json")).toEqual({ url: DEFAULT_RUNNER, token: "" });
  });
});

describe("parseEvents", () => {
  it("parses complete events and keeps the incomplete rest", () => {
    const [events, rest] = parseEvents(
      'id: 1\nevent: job.progress\ndata: {"done":3}\n\n:keep-alive\n\nid: 2\nevent: x\nda',
    );
    expect(events).toEqual([{ id: 1, kind: "job.progress", data: { done: 3 } }]);
    expect(rest).toBe("id: 2\nevent: x\nda");
  });
});
