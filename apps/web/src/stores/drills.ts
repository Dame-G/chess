import { computed, ref } from "vue";
import { defineStore } from "pinia";
import { client, ok } from "../api/client";
import type {
  Drill,
  DrillAnswerResult,
  DrillHint,
  DrillId,
  DrillOutcome,
  DrillsToday,
  HitRatePoint,
} from "../api/types";
import { nextQuestion, pips, questionOf, scopeQuery, sitting, streak, type Scope } from "../board/drill";
import { listen, reloadOnReconnect } from "./listen";
import { usePolicy } from "./policy";

/**
 * Drills: your mistakes as exercises. The set for today, the hit-rate trend, and the sitting on the board: a set of
 * drills (today's, or one theme's, or one opening line's) with one try at each. The server sends a drill as a question,
 * without its answer or the move you played; `answer` returns them with the line. A hint is asked for here and kept by
 * the server until the answer, so the page cannot take it back. Hidden, like all analysis, while the policy forbids it:
 * `available` is false and nothing starts.
 */
export const useDrills = defineStore("drills", () => {
  const policy = usePolicy();

  const today = ref<DrillsToday | null>(null);
  const trend = ref<HitRatePoint[]>([]);
  const loaded = ref(false);

  /** The drill on the board, and the answer once it was tried. */
  const active = ref<Drill | null>(null);
  const result = ref<DrillAnswerResult | null>(null);
  const answering = ref(false);
  /** The hints asked for at the drill on the board, the kind of move first. */
  const hints = ref<DrillHint[]>([]);
  const hinting = ref(false);
  /** This sitting: what it is of, the set it started with, and what came of each drill. */
  const scope = ref<Scope>({ type: "day" });
  const queue = ref<Drill[]>([]);
  const outcomes = ref(new Map<DrillId, DrillOutcome>());
  const results = ref(new Map<DrillId, DrillAnswerResult>());
  /** The drills answered, in order, for the streak and the summary. */
  const order = ref<DrillId[]>([]);
  /** The sitting is over and its summary is on show. */
  const finished = ref(false);

  const available = computed(() => policy.allowed);
  const question = computed(() => (active.value ? questionOf(active.value) : null));
  const progress = computed(() => sitting(queue.value, outcomes.value));
  const strip = computed(() => pips(queue.value, results.value, active.value?.id ?? null));
  const run = computed(() => streak(order.value, results.value));
  const answers = computed(() => order.value.map((id) => results.value.get(id)!).filter(Boolean));
  /** Whether a sitting is on the board now: a drill, or its summary. */
  const drilling = computed(() => available.value && (active.value !== null || finished.value));
  /** Drills that exist, to tell "none yet" from "none today". */
  const total = computed(() => today.value?.total ?? 0);
  /** What the header's Train button counts: the drills waiting today. */
  const waiting = computed(() => today.value?.drills.length ?? 0);

  /** Reads today's set: cheap, so the header can keep it. */
  async function loadToday() {
    today.value = await ok(client.GET("/api/drills/today"));
    loaded.value = true;
  }

  async function load() {
    const [t, tr] = await Promise.all([
      ok(client.GET("/api/drills/today")),
      ok(client.GET("/api/drills/trend", { params: { query: { window: 50 } } })),
    ]);
    today.value = t;
    trend.value = tr;
    loaded.value = true;
  }

  /** Puts a drill on the board, untried. */
  function show(d: Drill | null) {
    active.value = d;
    result.value = null;
    hints.value = [];
  }

  function begin(set: Drill[], of: Scope) {
    scope.value = of;
    queue.value = set;
    outcomes.value = new Map();
    results.value = new Map();
    order.value = [];
    finished.value = false;
  }

  /**
   * Starts a sitting: today's drills, or the drills of one theme or opening line. The first is on the board. `false`
   * when there is nothing to do.
   */
  async function start(of: Scope = { type: "day" }): Promise<boolean> {
    if (!available.value) return false;
    let set: DrillsToday;
    if (of.type === "day") {
      await load();
      set = today.value!;
    } else {
      set = await ok(client.GET("/api/drills/today", { params: { query: scopeQuery(of) } }));
    }
    begin([...set.drills], of);
    const first = nextQuestion(queue.value, new Set());
    show(first);
    return first !== null;
  }

  /**
   * Opens one drill by id, such as from the address `/?drill=7`, in today's set: a sitting that was open when the page
   * was reloaded starts over from what is still due.
   */
  async function open(id: DrillId): Promise<boolean> {
    if (!available.value) return false;
    if (!queue.value.some((x) => x.id === id)) {
      await load();
      const d = today.value?.drills.find((x) => x.id === id);
      if (!d) return false;
      begin([...(today.value?.drills ?? [])], { type: "day" });
    }
    show(queue.value.find((x) => x.id === id) ?? null);
    return active.value !== null;
  }

  /** Asks for the next hint at the drill on the board: the kind of move, then the piece. It counts against the answer. */
  async function hint(): Promise<DrillHint | undefined> {
    const d = active.value;
    if (!d || result.value || hinting.value || hints.value.length >= 2) return undefined;
    hinting.value = true;
    try {
      const h = await ok(client.POST("/api/drills/{id}/hint", { params: { path: { id: d.id } } }));
      if (active.value?.id !== d.id || result.value) return h;
      hints.value = [...hints.value, h];
      return h;
    } finally {
      hinting.value = false;
    }
  }

  /** Your one try: the move the board played. The answer comes back with the line. */
  async function answer(move: string): Promise<DrillAnswerResult | undefined> {
    const d = active.value;
    if (!d || result.value || answering.value) return undefined;
    answering.value = true;
    try {
      const r = await ok(client.POST("/api/drills/{id}/answer", { params: { path: { id: d.id } }, body: { move } }));
      if (active.value?.id !== d.id) return r;
      result.value = r;
      outcomes.value.set(d.id, r.outcome);
      results.value.set(d.id, r);
      order.value = [...order.value.filter((x) => x !== d.id), d.id];
      applyDrill(r.drill);
      return r;
    } finally {
      answering.value = false;
    }
  }

  /** The next drill of the sitting; `false` when the set is done (the board is left as it was). */
  function next(): boolean {
    const d = nextQuestion(queue.value, new Set(outcomes.value.keys()), active.value?.id ?? null);
    if (!d) return false;
    show(d);
    return true;
  }

  /** Ends the sitting where it stands: its summary is on show, over the last position. */
  function finish() {
    if (outcomes.value.size === 0) return leave();
    finished.value = true;
  }

  /** Leaves the sitting: the board returns to what it showed before. */
  function leave() {
    show(null);
    finished.value = false;
    queue.value = [];
    outcomes.value = new Map();
    results.value = new Map();
    order.value = [];
  }

  /** Makes drills from the weak spots of your reviewed games; answers how many are new. */
  async function make(limit?: number): Promise<number> {
    const made = await ok(client.POST("/api/drills", { body: { limit: limit ?? null } }));
    await load();
    return made.length;
  }

  function applyDrill(d: Drill) {
    if (active.value?.id === d.id && !result.value) active.value = d;
    const q = queue.value.findIndex((x) => x.id === d.id);
    if (q >= 0) queue.value[q] = d;
  }

  let timer: ReturnType<typeof setTimeout> | undefined;
  /** Reads the set and the trend again, soon: drills were made, answered or deleted. */
  function refresh() {
    if (!loaded.value) return;
    clearTimeout(timer);
    timer = setTimeout(() => void load().catch(() => {}), 400);
  }

  listen("drill.upserted", (d) => {
    applyDrill(d);
    refresh();
  });
  listen("drill.answered", refresh);
  listen("drill.deleted", (d) => {
    queue.value = queue.value.filter((x) => x.id !== d.id);
    if (active.value?.id === d.id) show(null);
    refresh();
  });
  reloadOnReconnect(async () => {
    if (loaded.value) await load();
  });

  return {
    today,
    trend,
    loaded,
    active,
    result,
    answering,
    hints,
    hinting,
    scope,
    queue,
    outcomes,
    results,
    order,
    finished,
    available,
    question,
    progress,
    strip,
    run,
    answers,
    drilling,
    total,
    waiting,
    loadToday,
    load,
    start,
    open,
    hint,
    answer,
    next,
    finish,
    leave,
    make,
  };
});
