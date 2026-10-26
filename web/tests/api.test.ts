import { describe, expect, it, vi } from "vitest";

vi.stubGlobal("location", { hostname: "127.0.0.1" });
const { initialConnection, DEFAULT_RUNNER } = await import("../src/api/runner");
const { followEvents, parseEvents } = await import("../src/api/events");

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

describe("followEvents", () => {
  const stream = (text: string, end = true) =>
    new Response(
      new ReadableStream({
        start(c) {
          c.enqueue(new TextEncoder().encode(text));
          if (end) c.close();
        },
      }),
    );

  it("starts from now, resumes from the newest id, and says when it may have missed events", async () => {
    const urls: string[] = [];
    const replies: (() => Promise<Response>)[] = [
      async () => stream("id: 7\nevent: a\ndata: 1\n\nid: 8\nevent: b\ndata: 2\n\n"),
      async () => Promise.reject(new TypeError("refused")),
      // The runner restarted: its ids begin again.
      async () => stream("id: 2\nevent: c\ndata: 3\n\n", false),
      async () => new Promise<Response>(() => {}),
    ];
    vi.stubGlobal("fetch", (input: string) => {
      urls.push(input);
      return replies.shift()!();
    });
    const seen: string[] = [];
    const stop = followEvents((e) => seen.push(`${e.id}:${e.kind}`), 0);
    await vi.waitFor(() => expect(seen).toContain("2:c"));
    stop();
    vi.unstubAllGlobals();

    expect(seen).toEqual(["7:a", "8:b", "0:runner.started", "2:c"]);
    expect(urls.map((u) => new URL(u, "http://x").searchParams.get("since"))).toEqual([
      String(Number.MAX_SAFE_INTEGER),
      "8",
      "8",
    ]);
  });
});
