<script setup lang="ts">
import { computed, onMounted, ref, watch } from "vue";
import { useToast } from "@eos-ui/vue";
import { EButton } from "@eos-ui/vue/components/EButton";
import { ETextarea } from "@eos-ui/vue/components/ETextarea";
import { ETextField } from "@eos-ui/vue/components/ETextField";
import type { Engine } from "../api/types";
import { bytes, formatNumber } from "../format";
import { addErrorField, canAnalyse, engineDetail, readOptions, setupLine, sourceOf } from "../sheets/engines";
import { useEngines } from "../stores/engines";
import { useWork } from "../stores/work";
import UiBadge from "../ui/UiBadge.vue";
import UiDialog from "../ui/UiDialog.vue";
import UiEmpty from "../ui/UiEmpty.vue";
import UiSection from "../ui/UiSection.vue";
import { useConfirm } from "../ui/useConfirm";

/**
 * Settings › Engines: every engine the app can play and analyse with (the built-ins, installed or not, and the ones
 * you added), which one analyses, and the way to add, remove and set them up. Setup says what it would download and
 * how much before anything is fetched; the download runs as work, which the header shows.
 */
const engines = useEngines();
const work = useWork();
const toast = useToast();
const confirm = useConfirm();

const failed = ref<string | null>(null);
async function read() {
  failed.value = null;
  try {
    await Promise.all([engines.load(), engines.loadPlan()]);
  } catch (e) {
    failed.value = e instanceof Error ? e.message : String(e);
  }
}
onMounted(read);

async function attempt(title: string, action: () => Promise<unknown>) {
  try {
    await action();
    return true;
  } catch (e) {
    toast.show({ title, message: e instanceof Error ? e.message : String(e), tone: "warning" });
    return false;
  }
}

const busy = ref<string | null>(null);
async function analyseWith(e: Engine) {
  busy.value = e.id;
  await attempt("The analysis engine did not change", () => engines.useForAnalysis(e.id));
  busy.value = null;
}

async function remove(e: Engine) {
  const yes = await confirm({
    title: `Remove ${e.name}?`,
    body: "It leaves the engines list; the program itself stays where it is on this machine. Games against it are kept.",
    confirmLabel: "Remove",
    danger: true,
  });
  if (!yes) return;
  busy.value = e.id;
  await attempt("The engine was not removed", () => engines.remove(e.id));
  busy.value = null;
}

// ---- Adding an engine

const adding = ref(false);
const name = ref("");
const path = ref("");
const optionsText = ref("");
/** What the server said when it would not add the engine, shown at the field it is about until that field changes. */
const nameError = ref<string | undefined>(undefined);
const pathError = ref<string | undefined>(undefined);
const saving = ref(false);
const options = computed(() => readOptions(optionsText.value));
watch(name, () => (nameError.value = undefined));
watch(path, () => (pathError.value = undefined));

function openAdd() {
  name.value = "";
  path.value = "";
  optionsText.value = "";
  nameError.value = pathError.value = undefined;
  adding.value = true;
}

async function add() {
  if (options.value.error) return;
  saving.value = true;
  nameError.value = pathError.value = undefined;
  try {
    const made = await engines.add({
      name: name.value.trim(),
      path: path.value.trim(),
      options: options.value.options,
    });
    adding.value = false;
    toast.show({ title: `${made.name} is in the list`, tone: "success" });
  } catch (e) {
    const message = e instanceof Error ? e.message : String(e);
    if (addErrorField(message) === "name") nameError.value = message;
    else pathError.value = message;
  } finally {
    saving.value = false;
  }
}

// ---- Setup

const plan = computed(() => engines.plan);
const settingUp = computed(() => [...work.running, ...work.queued].find((w) => w.kind === "setup"));
const starting = ref(false);
async function download() {
  const p = plan.value;
  if (!p?.downloads.length) return;
  const yes = await confirm({
    title: `Download ${p.bytes ? bytes(p.bytes) : "the engines"}?`,
    body: `${setupLine(p)}, from their GitHub releases into the data directory's tools folder.`,
    confirmLabel: "Download",
  });
  if (!yes) return;
  starting.value = true;
  const ok = await attempt("Setup did not start", () => engines.setup());
  starting.value = false;
  if (ok) toast.show({ title: "Downloading the engines", message: "The header shows how it goes.", tone: "info" });
}
</script>

<template>
  <div>
    <UiSection title="Engines">
      <template #aside>
        <EButton variant="outline" size="sm" @click="openAdd">Add engine</EButton>
      </template>
      <template #note>The analysis engine runs reviews, live evaluation, drills and the levels.</template>
      <UiEmpty v-if="failed" dense tone="error" title="The engines could not be read." :body="failed">
        <template #actions><EButton variant="quiet" size="sm" @click="read">Try again</EButton></template>
      </UiEmpty>
      <span v-else-if="!engines.loaded" class="skel block" aria-label="Reading the engines" />
      <ul v-else class="list" aria-label="Engines">
        <li v-for="e in engines.list" :key="e.id" class="row" :class="{ 'is-off': !e.installed }">
          <span class="what">
            <span class="name">
              {{ e.name }}
              <UiBadge v-if="e.analysis" tone="act">Analysis</UiBadge>
              <UiBadge v-if="e.rating != null">Plays at {{ formatNumber(e.rating) }}</UiBadge>
              <UiBadge v-if="e.kind === 'custom'" tone="info">Added</UiBadge>
              <UiBadge v-if="!e.installed" tone="warn">Not installed</UiBadge>
            </span>
            <span class="detail">{{ engineDetail(e) }}</span>
          </span>
          <span class="actions">
            <EButton
              v-if="canAnalyse(e) && !e.analysis"
              variant="quiet"
              size="sm"
              :loading="busy === e.id"
              @click="analyseWith(e)"
            >
              Use for analysis
            </EButton>
            <EButton v-if="e.kind === 'custom'" variant="quiet" size="sm" :disabled="busy === e.id" @click="remove(e)">
              Remove
            </EButton>
          </span>
        </li>
      </ul>
    </UiSection>

    <UiSection title="Setup">
      <template v-if="plan?.downloads.length" #aside>
        <EButton size="sm" :loading="starting" :disabled="!!settingUp" @click="download">
          Download{{ plan.bytes ? ` ${bytes(plan.bytes)}` : "" }}
        </EButton>
      </template>
      <span v-if="!plan && !failed" class="skel block" aria-label="Reading what setup would download" />
      <template v-else-if="plan">
        <p v-if="settingUp" class="status" role="status">{{ settingUp.title }} is running.</p>
        <p v-else-if="!plan.downloads.length" class="status">Everything setup installs is here.</p>
        <ul v-if="plan.downloads.length" class="list" aria-label="Downloads">
          <li v-for="d in plan.downloads" :key="d.url" class="row">
            <span class="what">
              <span class="name">{{ d.name }}</span>
              <span class="detail">{{ sourceOf(d.url).host }} · {{ sourceOf(d.url).file }}</span>
            </span>
            <span class="size num">{{ d.bytes ? bytes(d.bytes) : "" }}</span>
          </li>
          <li v-if="plan.downloads.length > 1 && plan.bytes" class="row total">
            <span class="what"><span class="name">Total</span></span>
            <span class="size num">{{ bytes(plan.bytes) }}</span>
          </li>
        </ul>
        <p v-for="n in plan.notes" :key="n" class="detail">{{ n }}</p>
      </template>
    </UiSection>

    <UiDialog
      v-model:open="adding"
      title="Add an engine"
      confirm-label="Add engine"
      :busy="saving"
      :confirm-disabled="!name.trim() || !path.trim() || !!options.error"
      @confirm="add"
    >
      <div class="form">
        <ETextField v-model="name" label="Name" autocomplete="off" required :error="nameError" />
        <ETextField
          v-model="path"
          label="Program"
          hint="The engine's executable on this machine: any UCI engine."
          autocomplete="off"
          required
          :error="pathError"
        />
        <ETextarea
          v-model="optionsText"
          label="UCI options"
          hint="One per line, as Name=value: Threads=4."
          :rows="3"
          :error="options.error"
        />
      </div>
    </UiDialog>
  </div>
</template>

<style scoped>
.list {
  display: grid;
  margin: 0;
  padding: 0;
  list-style: none;
}

.row {
  display: grid;
  grid-template-columns: minmax(0, 1fr) auto;
  align-items: center;
  gap: var(--sp-1) var(--sp-3);
  min-block-size: var(--h-row-2);
  padding-block: var(--sp-2);
  border-block-end: var(--e-stroke-hair) solid var(--c-line);
}

.row:last-child {
  border-block-end: 0;
}

.what {
  display: grid;
  gap: var(--sp-1);
  min-width: 0;
}

.name {
  display: flex;
  flex-wrap: wrap;
  align-items: center;
  gap: var(--sp-1) var(--sp-2);
  font: var(--t-body-strong);
}

.is-off .name {
  color: var(--c-ink-2);
}

.detail {
  margin: 0;
  color: var(--c-ink-2);
  font: var(--t-meta);
  overflow-wrap: anywhere;
}

.status {
  margin: 0 0 var(--sp-2);
  color: var(--c-ink-2);
  font: var(--t-body);
}

/* The actions share the end edge of every row, so a column of rows reads as a table. */
.actions {
  display: flex;
  flex-wrap: wrap;
  justify-content: flex-end;
  gap: var(--sp-1);
}

.size {
  font: var(--t-fig);
  text-align: end;
}

.total {
  border-block-start: var(--e-stroke-hair) solid var(--c-line-strong);
}

.total .name,
.total .size {
  font-weight: 700;
}

.form {
  display: grid;
  gap: var(--sp-3);
}

.block {
  display: block;
  block-size: var(--h-row-2);
}

@media (width < 30rem) {
  .row:has(.actions > *) {
    grid-template-columns: minmax(0, 1fr);
  }

  .actions {
    justify-content: flex-start;
    margin-inline-start: calc(-1 * var(--sp-2));
  }
}
</style>
