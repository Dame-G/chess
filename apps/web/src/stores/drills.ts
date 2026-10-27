import { computed, ref } from "vue";
import { defineStore } from "pinia";
import { client, ok } from "../api/client";
import type { Drill, DrillAnswerResult, DrillId, DrillOutcome, DrillsToday, HitRatePoint } from "../api/types";
import { nextQuestion, questionOf, sitting } from "../board/drill";
import { listen, reloadOnReconnect } from "./listen";
import { usePolicy } from "./policy";

/**
 * Drills: your mistakes as exercises. The set for today, the hit-rate trend, and the drill on the board with its one
 * try. The server sends a drill's answer along with it; this store keeps the drill on the board as a question
 * (`board/drill.ts`) until `answer` returns, and nothing else reads `best_moves`. Hidden, like all analysis, while the
 * policy forbids it: `available` is false and nothing starts.
 */
export const useDrills = defineStore("drills", () => {
  const policy = usePolicy();

  const today = ref<DrillsToday | null>(null);
  const all = ref<Drill[]>([]);
  const trend = ref<HitRatePoint[]>([]);
  const loaded = ref(false);

  /** The drill on the board, and the answer once it was tried. */
  const active = ref<Drill | null>(null);
  const result = ref<DrillAnswerResult | null>(null);
  const answering = ref(false);
  /** This sitting: the set it started with and what came of each drill. */
  const queue = ref<Drill[]>([]);
  const outcomes = ref(new Map<DrillId, DrillOutcome>());

  const available = computed(() => policy.allowed);
  const question = computed(() => (active.value ? questionOf(active.value) : null));
  const progress = computed(() => sitting(queue.value, outcomes.value));
  /** Whether a drill is on the board now. */
  const drilling = computed(() => available.value && active.value !== null);
  /** Drills that exist, to tell "none yet" from "none today". */
  const total = computed(() => all.value.length);

  async function load() {
    const [t, list, tr] = await Promise.all([
      ok(client.GET("/api/drills/today")),
      ok(client.GET("/api/drills")),
      ok(client.GET("/api/drills/trend", { params: { query: { window: 50 } } })),
    ]);
    today.value = t;
    all.value = list;
    trend.value = tr;
    loaded.value = true;
  }

  /** Puts a drill on the board, untried. */
  function show(d: Drill | null) {
    active.value = d;
    result.value = null;
  }

  /** Starts today's drills: the first on the board. `false` when there is nothing to do. */
  async function start(): Promise<boolean> {
    if (!available.value) return false;
    await load();
    queue.value = [...(today.value?.drills ?? [])];
    outcomes.value = new Map();
    const first = nextQuestion(queue.value, new Set());
    show(first);
    return first !== null;
  }

  /** Opens one drill by id, such as from the address `/?drill=7`. Starts a sitting of today's set when it is in it. */
  async function open(id: DrillId): Promise<boolean> {
    if (!available.value) return false;
    if (!loaded.value || !all.value.some((d) => d.id === id)) await load();
    const d = all.value.find((x) => x.id === id) ?? today.value?.drills.find((x) => x.id === id);
    if (!d) return false;
    if (!queue.value.some((x) => x.id === id)) {
      queue.value = [...(today.value?.drills ?? [d])];
      if (!queue.value.some((x) => x.id === id)) queue.value.push(d);
      outcomes.value = new Map();
    }
    show(d);
    return true;
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

  /** Leaves the drill: the board returns to what it showed before. */
  function leave() {
    show(null);
  }

  /** Makes drills from the weak spots of your reviewed games; answers how many are new. */
  async function make(limit?: number): Promise<number> {
    const made = await ok(client.POST("/api/drills", { body: { limit: limit ?? null } }));
    await load();
    return made.length;
  }

  function applyDrill(d: Drill) {
    const i = all.value.findIndex((x) => x.id === d.id);
    if (i >= 0) all.value[i] = d;
    else all.value.push(d);
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
    all.value = all.value.filter((x) => x.id !== d.id);
    queue.value = queue.value.filter((x) => x.id !== d.id);
    if (active.value?.id === d.id) show(null);
    refresh();
  });
  reloadOnReconnect(async () => {
    if (loaded.value) await load();
  });

  return {
    today,
    all,
    trend,
    loaded,
    active,
    result,
    answering,
    queue,
    outcomes,
    available,
    question,
    progress,
    drilling,
    total,
    load,
    start,
    open,
    answer,
    next,
    leave,
    make,
  };
});
