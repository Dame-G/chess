<script setup lang="ts">
import { computed, ref } from "vue";
import { EButton } from "@eos-ui/vue/components/EButton";
import { ENotice } from "@eos-ui/vue/components/ENotice";
import { ESpinner } from "@eos-ui/vue/components/ESpinner";
import { api } from "../../api/runner";
import { unwrap, useJobs, type Job } from "../../api/queries";

const props = defineProps<{ hasGames: boolean }>();

const { state: jobs } = useJobs();
const started = ref<number>();
const busy = ref(false);
const error = ref("");

const imports = computed(() => (jobs.value.data ?? []).filter((j) => j.spec.kind === "lichess_import"));
/** The import this page started, else the newest one. */
const job = computed<Job | undefined>(() => imports.value.find((j) => j.id === started.value) ?? imports.value[0]);
const running = computed(() => job.value?.status === "queued" || job.value?.status === "running");
const done = computed(() => imports.value.some((j) => j.status === "done") || props.hasGames);
const result = computed(() =>
  job.value?.status === "done" ? (job.value.result as { imported?: number; skipped?: number } | null) : null,
);
const sentence = computed(() => {
  const j = job.value;
  if (!j) return "";
  if (running.value) return "Fetching your games…";
  if (j.status === "failed") return j.error ?? "The import failed.";
  if (j.status === "done" && result.value) {
    const { imported = 0, skipped = 0 } = result.value;
    return `${imported === 1 ? "1 game" : `${imported} games`} added${skipped ? `, ${skipped} already here` : ""}.`;
  }
  return "";
});

async function start() {
  busy.value = true;
  error.value = "";
  try {
    started.value = (await unwrap(api.POST("/api/lichess/me/import", { body: {} }))).id;
  } catch (e) {
    error.value = (e as Error).message;
  } finally {
    busy.value = false;
  }
}
</script>

<template>
  <div class="import">
    <EButton variant="outline" :loading="busy || running" @click="start">
      {{ done ? "Fetch new games" : "Import my games" }}
    </EButton>
    <p v-if="running" class="say" role="status"><ESpinner size="sm" silent /> {{ sentence }}</p>
    <p v-else-if="sentence && job?.status === 'done'" class="say" role="status">{{ sentence }}</p>
    <ENotice v-else-if="sentence" tone="danger" title="Import failed" :role="false">{{ sentence }}</ENotice>
    <p v-else class="say">Your finished games on Lichess, to replay and study here.</p>
    <ENotice v-if="error" tone="danger" title="That did not work" :role="false">{{ error }}</ENotice>
  </div>
</template>

<style scoped>
.import {
  display: grid;
  justify-items: start;
  gap: var(--e-gap-base);
}

.say {
  display: flex;
  align-items: center;
  gap: var(--e-gap-snug);
  margin: 0;
  color: var(--e-muted);
  font: var(--e-type-small);
}
</style>
