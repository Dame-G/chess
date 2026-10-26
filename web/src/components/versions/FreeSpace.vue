<script setup lang="ts">
import { computed, ref, watch } from "vue";
import { useQueryCache } from "@pinia/colada";
import { EButton } from "@eos-ui/vue/components/EButton";
import { EDialog } from "@eos-ui/vue/components/EDialog";
import { ENotice } from "@eos-ui/vue/components/ENotice";
import { ENumberField } from "@eos-ui/vue/components/ENumberField";
import { ESwitch } from "@eos-ui/vue/components/ESwitch";
import { ESkeleton } from "@eos-ui/vue/components/ESkeleton";
import { api } from "../../api/runner";
import { unwrap } from "../../api/queries";
import type { components } from "../../api/schema";
import { bytesText } from "./versions";

type Result = components["schemas"]["PruneResult"];

/** Frees disk by deleting builds nothing uses. It always asks the runner what would go first. */
const open = defineModel<boolean>("open", { default: false });

const cache = useQueryCache();
const keepNewest = ref(true);
const older = ref<number | null>(null);
const preview = ref<Result>();
const loading = ref(false);
const busy = ref(false);
const error = ref("");
const done = ref<Result>();

const request = () => ({
  keep_latest_per_branch: keepNewest.value,
  older_than_days: older.value && older.value > 0 ? older.value : null,
});

let latest = 0;
async function look() {
  const mine = ++latest;
  loading.value = true;
  error.value = "";
  preview.value = undefined;
  try {
    const result = await unwrap(api.POST("/api/builds/prune", { body: { ...request(), dry_run: true } }));
    if (mine === latest) preview.value = result;
  } catch (e) {
    if (mine === latest) error.value = (e as Error).message;
  } finally {
    if (mine === latest) loading.value = false;
  }
}

watch(
  open,
  (now) => {
    if (!now) return;
    done.value = undefined;
    void look();
  },
  { immediate: true },
);
watch([keepNewest, older], () => {
  if (open.value && !done.value) void look();
});

async function free() {
  busy.value = true;
  error.value = "";
  try {
    done.value = await unwrap(api.POST("/api/builds/prune", { body: { ...request(), dry_run: false } }));
    void cache.invalidateQueries({ key: ["versions"] });
    void cache.invalidateQueries({ key: ["builds"] });
  } catch (e) {
    error.value = (e as Error).message;
  } finally {
    busy.value = false;
  }
}

const shown = computed(() => done.value ?? preview.value);
const builds = (n: number) => `${n} ${n === 1 ? "build" : "builds"}`;
</script>

<template>
  <EDialog
    v-model:open="open"
    title="Free disk space"
    description="Deletes the binaries of builds that nothing is using. Their records stay, so past results still read."
    size="md"
  >
    <div class="body">
      <ESwitch v-model="keepNewest" label="Keep each version's newest build" :disabled="!!done" />
      <ENumberField
        v-model="older"
        label="Only builds older than"
        size="sm"
        :min="0"
        :step="1"
        suffix="days"
        optional
        :disabled="!!done"
      />
      <ESkeleton v-if="loading" :lines="2" label="Checking what would be deleted" />
      <template v-else-if="shown">
        <ENotice v-if="done" tone="success" :title="`Freed ${bytesText(done.freed_bytes)}`" :role="false">
          Deleted {{ builds(done.deleted.length) }}.
        </ENotice>
        <ENotice
          v-else-if="shown.deleted.length"
          tone="info"
          :title="`Would free ${bytesText(shown.freed_bytes)}`"
          :role="false"
        >
          {{ builds(shown.deleted.length) }} would be deleted.
        </ENotice>
        <ENotice v-else tone="info" title="Nothing to delete" :role="false">
          Every build is either the newest of its version or too recent.
        </ENotice>
        <p v-if="shown.in_use.length" class="note">
          {{ builds(shown.in_use.length) }} would match but {{ shown.in_use.length === 1 ? "is" : "are" }} in use by a
          test, tune, match or the Lichess bot, and stay.
        </p>
      </template>
      <ENotice v-if="error" tone="danger" title="That did not work" :role="false">{{ error }}</ENotice>
    </div>
    <template #actions>
      <EButton
        v-if="!done"
        variant="danger"
        :loading="busy"
        :disabled="loading || !preview?.deleted.length"
        @click="free"
      >
        Delete {{ preview ? builds(preview.deleted.length) : "builds" }}
      </EButton>
      <EButton variant="outline" @click="open = false">{{ done ? "Close" : "Cancel" }}</EButton>
    </template>
  </EDialog>
</template>

<style scoped>
.body {
  display: grid;
  gap: var(--e-gap-base);
}

.note {
  margin: 0;
  color: var(--e-muted);
  font: var(--e-type-small);
}
</style>
