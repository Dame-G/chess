<script setup lang="ts">
import { computed, h, ref, watch } from "vue";
import { useRouter } from "vue-router";
import { EButton } from "@eos-ui/vue/components/EButton";
import { ECodeBlock } from "@eos-ui/vue/components/ECodeBlock";
import { EDescriptionList } from "@eos-ui/vue/components/EDescriptionList";
import { EDrawer } from "@eos-ui/vue/components/EDrawer";
import { ENotice } from "@eos-ui/vue/components/ENotice";
import { EProgress } from "@eos-ui/vue/components/EProgress";
import { EStamp } from "@eos-ui/vue/components/EStamp";
import { ETable } from "@eos-ui/vue/components/ETable";
import { useRunnerEvent } from "../api/events";
import { api, isRemote } from "../api/runner";
import { unwrap, useExperiments, useJobs, useLichess, useStatus, useTunes, type Job } from "../api/queries";
import { ago, failure, jobTone } from "../format";
import { useActivity } from "../activity";
import ConnectRunner from "./ConnectRunner.vue";

const open = defineModel<boolean>("open", { default: false });

const { state: status, refetch } = useStatus();
const { state: jobs } = useJobs();
const { state: experiments } = useExperiments();
const { state: tunes } = useTunes();

const { live } = useActivity();
const router = useRouter();
const { state: lichess } = useLichess();
/** The Lichess bot, when it is switched on: a line here, the controls on its own page. */
const bot = computed(() => {
  const l = lichess.value.data;
  if (!l?.online) return undefined;
  const who = l.account?.username ?? "The bot";
  if (l.state === "error") return `${who} is not connected to Lichess: ${l.error ?? "unknown error"}`;
  if (l.state === "connecting") return `${who} is connecting to Lichess`;
  return l.playing.length
    ? `${who} is playing ${l.playing.length === 1 ? "a game" : `${l.playing.length} games`} on Lichess`
    : `${who} is online on Lichess`;
});
function openBot() {
  open.value = false;
  void router.push("/lichess");
}
const running = computed(() => jobs.value.data?.find((j) => j.status === "running"));
const runningTune = computed(() => {
  const spec = running.value?.spec;
  return spec?.kind === "tune" ? tunes.value.data?.find((t) => t.id === spec.id) : undefined;
});
const queued = computed(() => (jobs.value.data ?? []).filter((j) => j.status === "queued").reverse());
const recent = computed(() => (jobs.value.data ?? []).filter((j) => j.finished_at).slice(0, 6));

const LINES = 14;

/** The last lines the running job printed: what it had already printed, then each line as it arrives. */
const log = ref<string[]>([]);
let logFor = 0;
watch(
  () => (open.value ? running.value?.id : undefined),
  async (id) => {
    logFor = id ?? 0;
    log.value = [];
    if (!id) return;
    const { data } = await api.GET("/api/jobs/{id}/log", { params: { path: { id } }, parseAs: "text" });
    if (id === logFor && data) log.value = [...data.trimEnd().split("\n").slice(-LINES), ...log.value].slice(-LINES);
  },
  { immediate: true },
);
useRunnerEvent("job.log", (data) => {
  const { id, line } = data as { id: number; line: string };
  if (id !== logFor) return;
  log.value = [...log.value.slice(-(LINES - 1)), line];
});

function describe(job: Job): string {
  if (job.spec.kind === "build") return `Build ${job.spec.ref}`;
  if (job.spec.kind === "lichess_import") return "Import Lichess games";
  if (job.spec.kind === "lichess_player_import") return `Import ${job.spec.username}'s games`;
  if (job.spec.kind === "review") return "Review games";
  if (job.spec.kind === "tune" || job.spec.kind === "apply_tune") {
    const id = job.spec.id;
    const name = tunes.value.data?.find((t) => t.id === id)?.name ?? `#${id}`;
    return job.spec.kind === "tune" ? `Tune ${name}` : `Apply tune ${name}`;
  }
  const id = job.spec.id;
  const e = experiments.value.data?.find((x) => x.id === id);
  return e ? `Test ${e.name}` : `Test ${id}`;
}

const outcome = {
  queued: "Queued",
  running: "Running",
  done: "Done",
  failed: "Failed",
  cancelled: "Stopped",
} as const;

async function cancel(job: Job) {
  await unwrap(api.DELETE("/api/jobs/{id}", { params: { path: { id: job.id } } }));
}

const gib = (bytes: number) => `${Math.round(bytes / 2 ** 30)} GB`;
const machine = computed(() => {
  const s = status.value.data;
  if (!s) return [];
  return [
    { label: "Processor", text: s.cpu.name.replace(/\((R|TM)\)/g, "").replace(/\s+/g, " ") },
    { label: "Cores", text: `${s.cpu.cores} cores, ${s.cpu.threads} threads, ${gib(s.cpu.memory)} memory` },
    {
      label: "Engine code",
      text: h(
        "span",
        { class: "num", style: { fontFamily: "var(--e-font-mono)", overflowWrap: "anywhere" } },
        s.reckless_dir,
      ),
      stamp: s.reckless_found ? "Found" : "Missing",
      stampTone: s.reckless_found ? ("success" as const) : ("danger" as const),
    },
    { label: "Runner", text: `version ${s.version}` },
  ];
});
</script>

<template>
  <EDrawer
    v-model:open="open"
    title="Runner"
    description="The program on your computer that builds engines and plays the test games, one job at a time."
    size="md"
  >
    <div class="runner">
      <template v-if="status.status === 'error'">
        <ENotice tone="warning" title="No runner connected" :role="false">
          <template v-if="isRemote">
            Start one with <code>chess serve</code> on your machine, then connect to it below. Your browser may ask to
            allow access to devices on your local network.
          </template>
          <template v-else>Start one with <code>chess serve</code>.</template>
        </ENotice>
      </template>
      <ConnectRunner v-if="isRemote" @connected="refetch()" />

      <section class="part" aria-labelledby="r-now">
        <h3 id="r-now" class="part-title">Doing now</h3>
        <div v-if="running" class="now">
          <p class="now-text">
            <b>{{ describe(running) }}</b>
            <span class="muted">started {{ ago(running.started_at ?? running.created_at) }}</span>
          </p>
          <EButton variant="outline" size="sm" @click="cancel(running)">Stop</EButton>
        </div>
        <EProgress
          v-if="runningTune?.iteration"
          :label="`Tuning ${runningTune.name}`"
          :value="runningTune.iteration"
          :max="runningTune.iterations"
          :value-text="`iteration ${runningTune.iteration}/${runningTune.iterations}`"
        />
        <p v-if="live" class="now now--live">
          <span class="now-text">
            <b>Live match</b>
            <span class="muted"
              >{{ live.white.name }} vs {{ live.black.name }}, started {{ ago(live.created_at) }}</span
            >
          </span>
        </p>
        <p v-if="bot" class="now now--live">
          <span class="now-text">
            <b>Lichess bot</b>
            <span class="muted">{{ bot }}</span>
          </span>
          <EButton variant="outline" size="sm" @click="openBot">Open</EButton>
        </p>
        <p v-if="!running && !live && !bot" class="muted">
          Nothing. Start a build or an experiment and it appears here.
        </p>
        <ECodeBlock v-if="running && log.length" :code="log.join('\n')" lang="text" label="Live output" wrap />
      </section>

      <section v-if="queued.length" class="part" aria-labelledby="r-next">
        <h3 id="r-next" class="part-title">Up next</h3>
        <ETable caption="Queued jobs" dense hide-caption>
          <tbody>
            <tr v-for="j in queued" :key="j.id">
              <td>{{ describe(j) }}</td>
              <td class="e-num"><EButton variant="quiet" size="sm" @click="cancel(j)">Remove</EButton></td>
            </tr>
          </tbody>
        </ETable>
      </section>

      <section v-if="recent.length" class="part" aria-labelledby="r-done">
        <h3 id="r-done" class="part-title">Recently finished</h3>
        <ETable caption="Finished jobs" dense hide-caption>
          <tbody>
            <tr v-for="j in recent" :key="j.id">
              <td>
                {{ describe(j) }}
                <span v-if="j.status === 'failed'" class="muted why" :title="j.error ?? undefined">{{
                  failure(j.error, j.spec.kind)
                }}</span>
              </td>
              <td class="muted when">{{ ago(j.finished_at!) }}</td>
              <td><EStamp size="sm" :tone="jobTone[j.status]" :label="outcome[j.status]" /></td>
            </tr>
          </tbody>
        </ETable>
      </section>

      <section v-if="machine.length" class="part" aria-labelledby="r-machine">
        <h3 id="r-machine" class="part-title">This machine</h3>
        <EDescriptionList :items="machine" />
      </section>
    </div>
  </EDrawer>
</template>

<style scoped>
.runner {
  display: grid;
  gap: var(--e-gap-section);
}

.part {
  display: grid;
  gap: var(--e-gap-base);
}

.when {
  white-space: nowrap;
}

.why {
  display: block;
}

.now {
  display: flex;
  margin: 0;
  align-items: center;
  justify-content: space-between;
  gap: var(--e-gap-loose);
  padding: var(--e-gap-base) var(--e-pad-control-sm);
  border: var(--e-stroke-hair) solid var(--e-border);
  background: var(--e-surface-tint);
}

.now-text {
  display: grid;
  gap: var(--e-gap-tight);
  min-inline-size: 0;
  margin: 0;
}

.muted {
  margin: 0;
  color: var(--e-muted);
  font: var(--e-type-small);
}
</style>
