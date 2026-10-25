<script setup lang="ts">
import { computed, ref } from "vue";
import { EButton } from "@eos-ui/vue/components/EButton";
import { ECodeBlock } from "@eos-ui/vue/components/ECodeBlock";
import { EDescriptionList } from "@eos-ui/vue/components/EDescriptionList";
import { EDrawer } from "@eos-ui/vue/components/EDrawer";
import { EGrid } from "@eos-ui/vue/components/EGrid";
import { EHeading } from "@eos-ui/vue/components/EHeading";
import { EMetric } from "@eos-ui/vue/components/EMetric";
import { ENotice } from "@eos-ui/vue/components/ENotice";
import { EStack } from "@eos-ui/vue/components/EStack";
import { EStamp } from "@eos-ui/vue/components/EStamp";
import { ETable } from "@eos-ui/vue/components/ETable";
import { useRunnerEvent } from "../api/events";
import { api, isRemote } from "../api/runner";
import { unwrap, useExperiments, useJobs, useStatus, type Job } from "../api/queries";
import { ago, jobTone } from "../format";
import ConnectRunner from "./ConnectRunner.vue";

const open = defineModel<boolean>("open", { default: false });

const { state: status, refetch } = useStatus();
const { state: jobs } = useJobs();
const { state: experiments } = useExperiments();

const running = computed(() => jobs.value.data?.find((j) => j.status === "running"));
const queued = computed(() => (jobs.value.data ?? []).filter((j) => j.status === "queued").reverse());
const recent = computed(() => (jobs.value.data ?? []).filter((j) => j.finished_at).slice(0, 6));

/** The last lines the running job printed, as they arrive. */
const log = ref<string[]>([]);
let logFor = 0;
useRunnerEvent("job.log", (data) => {
  const { id, line } = data as { id: number; line: string };
  if (id !== logFor) {
    logFor = id;
    log.value = [];
  }
  log.value = [...log.value.slice(-14), line];
});

function describe(job: Job): string {
  if (job.spec.kind === "build") return `Build ${job.spec.ref}`;
  const id = job.spec.id;
  const e = experiments.value.data?.find((x) => x.id === id);
  return e ? `Experiment: ${e.name}` : `Experiment ${id}`;
}

async function cancel(job: Job) {
  await unwrap(api.DELETE("/api/jobs/{id}", { params: { path: { id: job.id } } }));
}

const gib = (bytes: number) => `${Math.round(bytes / 2 ** 30)} GB`;
</script>

<template>
  <EDrawer
    v-model:open="open"
    title="Runner"
    description="The machine that builds, tests and analyses engines."
    size="md"
  >
    <EStack gap="section">
      <template v-if="status.status === 'error'">
        <ENotice tone="warning" title="No runner connected">
          <template v-if="isRemote">
            Start one with <code>chess serve</code> on your machine, then connect to it below. Your browser may ask to
            allow access to devices on your local network.
          </template>
          <template v-else>Start one with <code>chess serve</code>.</template>
        </ENotice>
      </template>
      <ConnectRunner v-if="isRemote" @connected="refetch()" />

      <template v-if="status.data">
        <EGrid min="8rem" gap="base">
          <EMetric label="Threads" :value="status.data.cpu.threads" :help="`${status.data.cpu.cores} cores`" />
          <EMetric label="Memory" :value="gib(status.data.cpu.memory)" />
          <EMetric label="Version" :value="status.data.version" />
        </EGrid>
        <EDescriptionList
          :items="[
            { label: 'Processor', text: status.data.cpu.name },
            {
              label: 'Reckless code',
              text: status.data.reckless_dir,
              stamp: status.data.reckless_found ? 'Found' : 'Missing',
              stampTone: status.data.reckless_found ? 'success' : 'danger',
            },
          ]"
        />
      </template>

      <EStack gap="snug">
        <EHeading :level="3">Working on</EHeading>
        <ENotice v-if="running" tone="info" :title="describe(running)" :role="false">
          Started {{ ago(running.started_at ?? running.created_at) }}.
          <template #actions>
            <EButton variant="outline" size="sm" @click="cancel(running)">Stop</EButton>
          </template>
        </ENotice>
        <p v-else class="quiet">Nothing. Builds and experiments run here one at a time.</p>
        <ECodeBlock v-if="running && log.length" :code="log.join('\n')" lang="text" label="Latest output" wrap />
      </EStack>

      <ETable v-if="queued.length" caption="Up next" dense>
        <tbody>
          <tr v-for="j in queued" :key="j.id">
            <td>{{ describe(j) }}</td>
            <td class="e-num"><EButton variant="quiet" size="sm" @click="cancel(j)">Remove</EButton></td>
          </tr>
        </tbody>
      </ETable>

      <ETable v-if="recent.length" caption="Recently finished" dense>
        <tbody>
          <tr v-for="j in recent" :key="j.id">
            <td>
              {{ describe(j) }}
              <span v-if="j.error" class="quiet">: {{ j.error }}</span>
            </td>
            <td>{{ ago(j.finished_at!) }}</td>
            <td><EStamp size="sm" :tone="jobTone[j.status]" :label="j.status" /></td>
          </tr>
        </tbody>
      </ETable>
    </EStack>
  </EDrawer>
</template>

<style scoped>
.quiet {
  margin: 0;
  color: var(--e-muted);
}
</style>
