import { computed } from "vue";
import { useExperiment, useExperiments, useJobs, useMatches, useStatus, useTest } from "./api/queries";
import { forecast, stageLabel } from "./format";

/** What the runner is doing, in a few words: for the status bar and the overview. */
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
  // A live engine match runs beside the queue, so it is reported on its own.
  const { state: matches } = useMatches();
  const live = computed(() => matches.value.data?.find((m) => m.status === "running"));
  const liveName = computed(() => (live.value ? `${live.value.white.name} vs ${live.value.black.name}` : ""));

  const queued = computed(() => (jobs.value.data ?? []).filter((j) => j.status === "queued").length);

  // The match being played now, for its game count and progress.
  const { state: detail } = useExperiment(() => experiment.value?.id ?? 0);
  const matchId = computed(() => detail.value.data?.tests.find((t) => t.running)?.id ?? 0);
  const { state: match } = useTest(matchId);
  const playing = computed(() => (matchId.value > 0 ? match.value.data?.test : undefined));

  const summary = computed(() => {
    if (status.value.status === "error") return { state: "offline", label: "Not connected", detail: [] } as const;
    if (status.value.status === "pending" && !status.value.data)
      return { state: "pending", label: "Connecting", detail: [] } as const;
    const job = running.value;
    if (!job && live.value) return { state: "busy", label: "Live match", detail: [liveName.value] } as const;
    if (!job) return { state: "idle", label: "Idle", detail: [] } as const;
    if (job.spec.kind === "build") return { state: "busy", label: `Building ${job.spec.ref}`, detail: [] } as const;
    const e = experiment.value;
    const stage = e?.current ? stageLabel(e.current) : "Preparing";
    if (!e) return { state: "busy", label: "Testing", detail: [] } as const;
    const detailParts: string[] = [e.name];
    const t = playing.value;
    let progress: number | undefined;
    if (t && t.games > 0) {
      detailParts.push(`${t.games.toLocaleString("en")} games`);
      progress = forecast(t)?.progress;
      if (progress != null) detailParts.push(`${Math.round(progress * 100)}%`);
    }
    if (live.value) detailParts.push(`live match: ${liveName.value}`);
    return { state: "busy", label: stage, detail: detailParts } as const;
  });

  return { status, running, experiment, live, queued, summary };
}
