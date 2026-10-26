import { ref } from "vue";
import { useRunnerEvent } from "../../api/events";

/** How far a review has got: the runner reports after every game. */
export interface ReviewProgress {
  job: number;
  done: number;
  total: number;
  /** Waiting for a game of yours on Lichess to end. */
  paused: boolean;
}

export function useReviewProgress() {
  const progress = ref<ReviewProgress>();
  useRunnerEvent("review.progress", (data) => {
    const p = data as ReviewProgress;
    progress.value = { job: p.job, done: p.done, total: p.total, paused: p.paused };
  });
  return progress;
}
