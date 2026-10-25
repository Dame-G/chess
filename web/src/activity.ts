import { computed } from "vue";
import { useExperiments, useJobs, useStatus } from "./api/queries";
import { stageLabel } from "./format";

/** What the runner is doing, in a few words: for the header button and the overview. */
export function useActivity() {
  const { state: status } = useStatus();
  const { state: jobs } = useJobs();
  const { state: experiments } = useExperiments();

  const running = computed(() => jobs.value.data?.find((j) => j.status === "running"));
  const experiment = computed(() => {
    const job = running.value;
    if (job?.spec.kind !== "experiment") return undefined;
    const id = job.spec.id;
    return experiments.value.data?.find((e) => e.id === id);
  });
  const queued = computed(() => (jobs.value.data ?? []).filter((j) => j.status === "queued").length);

  const summary = computed(() => {
    if (status.value.status === "error") return { state: "offline", label: "Not connected" } as const;
    if (status.value.status === "pending" && !status.value.data)
      return { state: "pending", label: "Connecting" } as const;
    const job = running.value;
    if (!job) return { state: "idle", label: "Idle" } as const;
    if (job.spec.kind === "build") return { state: "busy", label: `Building ${job.spec.ref}` } as const;
    const e = experiment.value;
    const stage = e?.current ? stageLabel(e.current) : "Preparing";
    return { state: "busy", label: e ? `${stage}: ${e.name}` : "Testing" } as const;
  });

  return { status, running, experiment, queued, summary };
}
