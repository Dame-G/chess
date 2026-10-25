<script setup lang="ts">
import { computed, ref } from "vue";
import { EButton } from "@eos-ui/vue/components/EButton";
import { ECodeBlock } from "@eos-ui/vue/components/ECodeBlock";
import { EForm } from "@eos-ui/vue/components/EForm";
import { ENotice } from "@eos-ui/vue/components/ENotice";
import { EPageHeader } from "@eos-ui/vue/components/EPageHeader";
import { EStack } from "@eos-ui/vue/components/EStack";
import { EStamp } from "@eos-ui/vue/components/EStamp";
import { ETable } from "@eos-ui/vue/components/ETable";
import { ETextField } from "@eos-ui/vue/components/ETextField";
import { useRunnerEvent } from "../api/events";
import { api } from "../api/runner";
import { unwrap, useBuilds, useJobs, type Job } from "../api/queries";

const { state: builds } = useBuilds();
const { state: jobs } = useJobs();

const refname = ref("main");
const submitting = ref(false);
const error = ref("");

async function build() {
  submitting.value = true;
  error.value = "";
  try {
    await unwrap(api.POST("/api/builds", { body: { ref: refname.value } }));
  } catch (e) {
    error.value = (e as Error).message;
  } finally {
    submitting.value = false;
  }
}

/** The job running now, if any, with the tail of its output as it arrives. */
const running = computed(() => jobs.value.data?.find((j) => j.status === "running"));
const log = ref<string[]>([]);
useRunnerEvent("job.log", (data) => {
  const { id, line } = data as { id: number; line: string };
  if (id !== running.value?.id) log.value = [];
  log.value = [...log.value.slice(-11), line];
});

const recent = computed(() => (jobs.value.data ?? []).filter((j) => j.status !== "running").slice(0, 5));

function describe(job: Job) {
  return job.spec.kind === "build" ? `Build ${job.spec.ref}` : job.spec.kind;
}

const tone = { queued: "neutral", running: "info", done: "success", failed: "danger", cancelled: "neutral" } as const;

async function cancel(job: Job) {
  await unwrap(api.DELETE("/api/jobs/{id}", { params: { path: { id: job.id } } }));
}
</script>

<template>
  <EStack gap="section">
    <EPageHeader
      title="Engines"
      description="Builds of the Reckless checkout. Each is benched once; its node count must match the commit's Bench line."
    />

    <EForm @submit="build">
      <ETextField
        v-model="refname"
        label="Branch, tag or commit"
        hint="Any ref of the checkout, such as main, upstream/main or a commit."
        required
        :error="error"
      />
      <EButton type="submit" :loading="submitting">Build</EButton>
    </EForm>

    <EStack v-if="running" gap="snug">
      <ENotice tone="info" :title="`${describe(running)}: running`" :role="false">
        <EButton variant="outline" size="sm" @click="cancel(running)">Cancel</EButton>
      </ENotice>
      <ECodeBlock v-if="log.length" :code="log.join('\n')" lang="text" />
    </EStack>

    <ETable
      caption="Builds"
      :loading="builds.status === 'pending'"
      :empty="builds.data?.length === 0"
      empty-title="No builds yet"
      empty-text="Build main to start."
    >
      <thead>
        <tr>
          <th scope="col">Build</th>
          <th scope="col">Commit</th>
          <th scope="col">Subject</th>
          <th scope="col" class="e-num">Bench</th>
          <th scope="col" class="e-num">Speed</th>
        </tr>
      </thead>
      <tbody>
        <tr v-for="b in builds.data" :key="b.id">
          <th scope="row">{{ b.id }}</th>
          <td>
            <code>{{ b.commit.slice(0, 8) }}</code> {{ b.ref }}
          </td>
          <td>{{ b.subject }}</td>
          <td class="e-num">
            {{ b.bench.toLocaleString() }}
            <EStamp
              v-if="b.bench_expected != null"
              size="sm"
              :tone="b.bench_expected === b.bench ? 'success' : 'danger'"
              :label="b.bench_expected === b.bench ? 'Matches' : 'Differs'"
            />
          </td>
          <td class="e-num">{{ (b.nps / 1000).toFixed(0) }}k nps</td>
        </tr>
      </tbody>
    </ETable>

    <ETable v-if="recent.length" caption="Recent jobs" dense>
      <thead>
        <tr>
          <th scope="col">Job</th>
          <th scope="col">What</th>
          <th scope="col">Status</th>
        </tr>
      </thead>
      <tbody>
        <tr v-for="j in recent" :key="j.id">
          <th scope="row">{{ j.id }}</th>
          <td>
            {{ describe(j) }}<template v-if="j.error">: {{ j.error }}</template>
          </td>
          <td><EStamp size="sm" :tone="tone[j.status]" :label="j.status" /></td>
        </tr>
      </tbody>
    </ETable>
  </EStack>
</template>
