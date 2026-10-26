import { computed } from "vue";
import {
  useExperiment,
  useExperiments,
  useJobs,
  useLichess,
  useMatches,
  useStatus,
  useTest,
  useTunes,
} from "./api/queries";
import { forecast, stageLabel } from "./format";
import { opponentOf } from "./lichess";

/** What the runner is doing, in a few words: for the status bar and the overview. */
export function useActivity() {
  const { state: status } = useStatus();
  const { state: jobs } = useJobs();
  const { state: experiments } = useExperiments();
  const { state: tunes } = useTunes();

  const running = computed(() => jobs.value.data?.find((j) => j.status === "running"));
  const experiment = computed(() => {
    const job = running.value;
    if (job?.spec.kind !== "experiment") return undefined;
    const id = job.spec.id;
    return experiments.value.data?.find((e) => e.id === id);
  });
  // A live engine match runs beside the queue, so it is reported on its own.
  const { state: matches } = useMatches();
  const live = computed(() => matches.value.data?.find((m) => m.status === "running" && !m.lichess));
  // The Lichess bot plays beside everything else: it adds a word to the status, it never replaces it.
  const { state: lichess } = useLichess();
  const bot = computed(() => {
    const l = lichess.value.data;
    if (!l?.online) return undefined;
    const game = matches.value.data?.find((m) => m.status === "running" && m.lichess);
    if (game) return { playing: true, text: `vs ${opponentOf(game).player.name}` } as const;
    if (l.state === "error") return { playing: false, text: "Lichess bot not connected" } as const;
    if (l.state === "connecting") return { playing: false, text: "Lichess bot connecting" } as const;
    if (l.seek && l.seek.mode !== "off") return { playing: false, text: "Lichess · looking for a game" } as const;
    return { playing: false, text: "Lichess bot online" } as const;
  });
  const liveName = computed(() => (live.value ? `${live.value.white.name} vs ${live.value.black.name}` : ""));

  const queued = computed(() => (jobs.value.data ?? []).filter((j) => j.status === "queued").length);

  // The match being played now, for its game count and progress.
  const { state: detail } = useExperiment(() => experiment.value?.id ?? 0);
  const matchId = computed(() => detail.value.data?.tests.find((t) => t.running)?.id ?? 0);
  const { state: match } = useTest(matchId);
  const playing = computed(() => (matchId.value > 0 ? match.value.data?.test : undefined));

  const base = computed(() => {
    if (status.value.status === "error") return { state: "offline", label: "Not connected", detail: [] } as const;
    if (status.value.status === "pending" && !status.value.data)
      return { state: "pending", label: "Connecting", detail: [] } as const;
    const job = running.value;
    if (!job && live.value) return { state: "busy", label: "Live match", detail: [liveName.value] } as const;
    if (!job) return { state: "idle", label: "Idle", detail: [] } as const;
    if (job.spec.kind === "build") return { state: "busy", label: `Building ${job.spec.ref}`, detail: [] } as const;
    if (job.spec.kind === "tune" || job.spec.kind === "apply_tune") {
      const id = job.spec.id;
      const t = tunes.value.data?.find((x) => x.id === id);
      if (job.spec.kind === "apply_tune")
        return { state: "busy", label: "Applying tune", detail: t ? [t.name] : [] } as const;
      const progress = t?.iteration ? `iteration ${t.iteration}/${t.iterations}` : "starting";
      return { state: "busy", label: "Tuning", detail: t ? [t.name, progress] : [progress] } as const;
    }
    const e = experiment.value;
    if (!e) return { state: "busy", label: "Testing", detail: [] } as const;
    const stage = e.current ? stageLabel(e.current) : "Preparing";
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

  const summary = computed(() => {
    const b = base.value;
    const x = bot.value;
    if (!x || b.state === "offline" || b.state === "pending") return b;
    if (b.state === "idle")
      return x.playing
        ? ({ state: "busy", label: "Playing on Lichess", detail: [x.text] } as const)
        : ({ state: "idle", label: x.text, detail: [] } as const);
    return { ...b, detail: [...b.detail, x.playing ? `Lichess ${x.text}` : x.text] };
  });

  return { live, queued, summary };
}
