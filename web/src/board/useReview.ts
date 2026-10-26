/** A finished game's review: loaded when there is one, asked for when there is not, and followed while it runs. */
import { computed, ref, watch, type Ref } from "vue";
import { useQuery, useQueryCache } from "@pinia/colada";
import { api } from "../api/runner";
import { unwrap, useJobs } from "../api/queries";
import { useRunnerEvent } from "../api/events";
import type { Review } from "./review";

interface Progress {
  job: number;
  done: number;
  total: number;
  match: number;
  review: unknown;
  paused: boolean;
}

/**
 * `id` is the match; `enabled` is whether its review may be fetched at all (a finished game, and no game of yours on at
 * Lichess). A game with no review answers 404, which is "not reviewed", never an error.
 */
export function useReview(id: Ref<number>, enabled: Ref<boolean>) {
  const cache = useQueryCache();
  const query = useQuery({
    key: () => ["review", id.value],
    enabled: () => enabled.value && id.value > 0,
    query: async () => {
      const { data, response } = await api.GET("/api/matches/{id}/review", { params: { path: { id: id.value } } });
      // A runner that answers without a review in it has none to show.
      if (data && "plies" in data) return data as Review;
      if (response.status === 404) return null;
      throw new Error(`${response.status} ${response.statusText}`);
    },
    // A review never changes once made, so a visit does not ask again.
    staleTime: 60_000,
  });
  const review = computed(() => query.state.value.data ?? null);
  const loading = computed(() => enabled.value && query.state.value.status === "pending");

  // Whether a review job covering this game is queued or running, from the jobs list (so a game in someone else's batch
  // shows it too), and how far the runner has got.
  const { state: jobs } = useJobs();
  const own = ref<number>();
  const queued = computed(() =>
    (jobs.value.data ?? []).find(
      (j) =>
        (j.status === "queued" || j.status === "running") &&
        j.spec.kind === "review" &&
        j.spec.matches.includes(id.value),
    ),
  );
  const asked = ref(false);
  const progress = ref<Progress>();
  const paused = ref(false);
  const error = ref("");
  const busy = computed(() => asked.value || !!queued.value);
  watch(id, () => {
    asked.value = false;
    progress.value = undefined;
    paused.value = false;
    error.value = "";
    own.value = undefined;
  });

  function reload() {
    cache.invalidateQueries({ key: ["review", id.value] });
    cache.invalidateQueries({ key: ["matches"] });
  }

  useRunnerEvent("review.progress", (d) => {
    const p = d as Progress;
    if (queued.value?.id === p.job || own.value === p.job) {
      progress.value = p;
      paused.value = p.paused;
    }
    if (p.match === id.value && p.review) {
      asked.value = false;
      reload();
    }
  });
  useRunnerEvent("job.updated", (d) => {
    const j = d as { id: number; status: string; error?: string | null };
    if (j.id !== own.value) return;
    if (j.status === "done" || j.status === "failed" || j.status === "cancelled") {
      asked.value = false;
      progress.value = undefined;
      if (j.status !== "done") error.value = j.error ?? "The review did not finish.";
      reload();
    }
  });

  async function start() {
    error.value = "";
    asked.value = true;
    try {
      const job = await unwrap(api.POST("/api/reviews", { body: { matches: [id.value] } }));
      own.value = job.id;
    } catch (e) {
      asked.value = false;
      error.value = (e as Error).message;
    }
  }

  return { review, loading, busy, paused, progress, error, start };
}
