<script setup lang="ts">
import { computed, onMounted, reactive, ref } from "vue";
import { useRoute } from "vue-router";
import { useToast } from "@eos-ui/vue";
import { EButton } from "@eos-ui/vue/components/EButton";
import { ECopyButton } from "@eos-ui/vue/components/ECopyButton";
import { EForm } from "@eos-ui/vue/components/EForm";
import { ENotice } from "@eos-ui/vue/components/ENotice";
import { EPasswordField } from "@eos-ui/vue/components/EPasswordField";
import { ETextField } from "@eos-ui/vue/components/ETextField";
import { ESwitch } from "@eos-ui/vue/components/ESwitch";
import { playMoveSound, soundOn, soundVolume } from "../board/sound";
import { connect, connection } from "../api/client";
import type { ConnectionStatus, HealthItem } from "../api/types";
import { formatDate } from "../format";
import EnginesSettings from "../parts/EnginesSettings.vue";
import PathText from "../parts/PathText.vue";
import { choice, type Choice } from "../scheme";
import { useConnections } from "../stores/connections";
import { useMachine } from "../stores/machine";
import { usePolicy } from "../stores/policy";
import { healthTone } from "../ui/badge";
import UiBadge from "../ui/UiBadge.vue";
import UiEmpty from "../ui/UiEmpty.vue";
import UiField from "../ui/UiField.vue";
import UiKeyValue from "../ui/UiKeyValue.vue";
import UiKv from "../ui/UiKv.vue";
import UiLive from "../ui/UiLive.vue";
import UiSection from "../ui/UiSection.vue";
import UiSeg from "../ui/UiSeg.vue";
import UiSheetPage from "../ui/UiSheetPage.vue";
import { useConfirm } from "../ui/useConfirm";

/**
 * Settings: your Lichess login, the engines (the list, the analysis engine, adding one, setup's downloads), the
 * machine (cores and the data directory), its health (what the app needs and whether it is there, failures first), and how the app looks, with the token the hosted app needs. Logging out asks first.
 * What cannot be read says so, with a way to try again, rather than staying a placeholder.
 */
const policy = usePolicy();
const connections = useConnections();
const machineStore = useMachine();
const toast = useToast();
const confirm = useConfirm();

/** The tab to open on: `?tab=engines` (New game links there for an engine not installed yet), else Accounts. */
const TABS = ["accounts", "engines", "machine", "health", "appearance"];
const asked = useRoute().query["tab"];
const tab = ref(typeof asked === "string" && TABS.includes(asked) ? asked : "accounts");

/** Sounds: three volumes (the control is the same as the theme's), each heard once when chosen. */
const volumes = [
  { value: 0.25, label: "Quiet" },
  { value: 0.5, label: "Medium" },
  { value: 0.9, label: "Loud" },
];
const volume = computed({
  get: () =>
    volumes.reduce((a, b) => (Math.abs(b.value - soundVolume.value) < Math.abs(a.value - soundVolume.value) ? b : a))
      .value,
  set: (v: number) => {
    soundVolume.value = v;
    playMoveSound("capture");
  },
});
const reducedMotion = typeof matchMedia === "function" && matchMedia("(prefers-reduced-motion: reduce)").matches;
const themes: { value: Choice; label: string }[] = [
  { value: "system", label: "System" },
  { value: "light", label: "Light" },
  { value: "dark", label: "Dark" },
];

/** What could not be read, by part, so each says so and can be tried again on its own. */
const failed = reactive({ machine: false, health: false, token: false });
const loaders = {
  machine: () => machineStore.loadMachine(),
  health: () => machineStore.loadHealth(),
  token: () => machineStore.loadToken(),
};
async function load(key: keyof typeof loaders) {
  failed[key] = false;
  try {
    await loaders[key]();
  } catch {
    failed[key] = true;
  }
}
onMounted(() => {
  for (const key of Object.keys(loaders) as (keyof typeof loaders)[]) void load(key);
});

async function attempt<T>(title: string, action: () => Promise<T>): Promise<T | undefined> {
  try {
    return await action();
  } catch (e) {
    toast.show({ title, message: e instanceof Error ? e.message : String(e), tone: "warning" });
    return undefined;
  }
}

// ---- Lichess

const you = computed(() => connections.you);
type Tone = "win" | "info" | "loss" | "neutral";
const stateOf = (c: ConnectionStatus): { word: string; tone: Tone } =>
  c.state === "online"
    ? { word: "Connected", tone: "win" }
    : c.state === "connecting"
      ? { word: "Connecting", tone: "info" }
      : c.state === "error"
        ? { word: "Cannot reach Lichess", tone: "loss" }
        : { word: "Offline", tone: "neutral" };
const accountName = (c: ConnectionStatus) =>
  c.account ? `${c.account.username}${c.account.title ? ` (${c.account.title})` : ""}` : "Linked";

const loggingIn = ref(false);
async function login() {
  loggingIn.value = true;
  const r = await attempt("Login did not start", () => connections.login());
  if (r) window.location.assign(r.url);
  else loggingIn.value = false;
}

async function logout() {
  const yes = await confirm({
    title: "Log out of Lichess?",
    body: "Your login is revoked at Lichess and forgotten here. Games you are playing there carry on. You can log in again at any time.",
    confirmLabel: "Log out",
    danger: true,
  });
  if (!yes) return;
  await attempt("Logging out did not work", () => connections.logout());
}

// ---- Machine

const machine = computed(() => machineStore.machine);
// ---- Health

/** Health, failures first. */
const ORDER = { fail: 0, warn: 1, info: 2, ok: 3 } as const;
const healthItems = computed(() =>
  (machineStore.health?.items ?? []).toSorted((a, b) => ORDER[a.level] - ORDER[b.level]),
);
const problems = computed(() => healthItems.value.filter((i) => i.level !== "ok").length);
const isOk = (i: HealthItem) => i.level === "ok";
/** The detail without the title it starts with: "Disk 212 GB free" under "Disk" says "212 GB free". */
const detailOf = (i: HealthItem) =>
  i.detail.toLowerCase().startsWith(i.title.toLowerCase()) ? i.detail.slice(i.title.length).trim() : i.detail;
// ---- This page and its server

const example = "http://127.0.0.1:7979";
const server = connection.url || location.origin;

/** Only a page from another origin (the hosted site) pairs with a server; one the server serves needs nothing. */
const remote = computed(() => connection.url !== "");
const serverUrl = ref(connection.url);
const token = ref("");
const urlError = computed(() =>
  serverUrl.value && !/^https?:\/\/[^\s/]+\S*$/.test(serverUrl.value.trim())
    ? "Start with http:// or https://, as in http://127.0.0.1:7979."
    : undefined,
);
function pair() {
  connect(serverUrl.value.trim(), token.value.trim());
  location.reload();
}

const tabs = computed(() => [
  { id: "accounts", label: "Accounts" },
  { id: "engines", label: "Engines" },
  { id: "machine", label: "This machine" },
  { id: "health", label: "Health", ...(problems.value ? { count: problems.value } : {}) },
  { id: "appearance", label: "Appearance" },
]);
</script>

<template>
  <UiSheetPage v-model:tab="tab" title="Settings" :tabs="tabs">
    <template #tab-accounts>
      <div>
        <UiSection title="Your account">
          <template v-if="connections.loaded && you.configured" #aside>
            <UiBadge :tone="stateOf(you).tone" dot>{{ stateOf(you).word }}</UiBadge>
          </template>
          <span v-if="!connections.loaded" class="skel block" aria-label="Reading your Lichess account" />
          <template v-else-if="you.configured">
            <UiKeyValue dense>
              <UiKv label="Account">{{ accountName(you) }}</UiKv>
              <UiKv v-if="you.state === 'error' && you.error" label="Problem" tone="loss">{{ you.error }}</UiKv>
              <UiKv v-if="you.you?.expires_at" label="Login ends">{{ formatDate(you.you.expires_at) }}</UiKv>
            </UiKeyValue>
            <ENotice v-if="you.you?.missing_scopes.length" tone="warning" title="The login is missing a permission">
              It lacks {{ you.you.missing_scopes.join(", ") }}. Log in again to give it.
            </ENotice>
            <div class="actions"><EButton variant="outline" size="sm" @click="logout">Log out</EButton></div>
          </template>
          <UiEmpty v-else dense title="Not connected." body="Log in to play on Lichess from here.">
            <template #actions>
              <EButton size="sm" :loading="loggingIn" @click="login">Log in with Lichess</EButton>
            </template>
          </UiEmpty>
        </UiSection>
      </div>
    </template>

    <template #tab-engines>
      <EnginesSettings />
    </template>

    <template #tab-machine>
      <div>
        <UiSection title="Cores and data">
          <UiEmpty v-if="failed.machine" dense title="The machine could not be read.">
            <template #actions>
              <EButton variant="quiet" size="sm" @click="load('machine')">Try again</EButton>
            </template>
          </UiEmpty>
          <span v-else-if="!machine" class="skel block" aria-label="Reading the machine" />
          <UiKeyValue v-else dense>
            <UiKv label="Cores">{{ policy.status ? policy.status.cores : "Unknown" }}</UiKv>
            <UiKv label="Data" copy><PathText :text="machine.data_dir" /></UiKv>
          </UiKeyValue>
        </UiSection>
      </div>
    </template>

    <template #tab-health>
      <div>
        <UiSection title="Checks">
          <UiEmpty v-if="failed.health" dense title="The health check did not run.">
            <template #actions><EButton variant="quiet" size="sm" @click="load('health')">Try again</EButton></template>
          </UiEmpty>
          <span v-else-if="!machineStore.health" class="skel block" aria-label="Checking the machine" />
          <ul v-else class="health">
            <li v-for="item in healthItems" :key="item.key" class="item" :class="{ 'item--ok': isOk(item) }">
              <span class="mark">
                <UiLive v-if="isOk(item)" tone="win" :label="healthTone.ok.word" />
                <UiBadge v-else :tone="healthTone[item.level].tone">{{ healthTone[item.level].word }}</UiBadge>
              </span>
              <span class="what">
                <span class="title">{{ item.title }}</span>
                <span class="detail">{{ isOk(item) ? detailOf(item) : item.detail }}</span>
                <span v-if="item.action" class="fix">
                  <span class="fix-label">{{ item.action.label }}</span>
                  <span class="fix-line">
                    <code class="fix-command">{{ item.action.command }}</code>
                    <ECopyButton :text="item.action.command" :label="`Copy the command for ${item.title}`" size="sm" />
                  </span>
                </span>
              </span>
            </li>
          </ul>
        </UiSection>
      </div>
    </template>

    <template #tab-appearance>
      <div>
        <UiSection title="Theme">
          <UiSeg v-model="choice" label="Theme" :options="themes" />
        </UiSection>

        <UiSection title="Sounds">
          <ESwitch
            v-model="soundOn"
            label="Play sounds"
            description="A soft knock for each move, a lower one for captures, a ping for check, a chime when a game ends and ticks when your clock runs low."
          />
          <UiField v-if="soundOn" label="Volume" group>
            <UiSeg v-model="volume" label="Volume" :options="volumes" />
          </UiField>
          <p v-if="reducedMotion" class="muted">
            Your system asks for less motion, so sounds start off until you turn them on.
          </p>
        </UiSection>

        <UiSection title="The hosted app">
          <UiKeyValue dense>
            <UiKv label="Server" copy><PathText :text="server" /></UiKv>
            <UiKv label="Version">{{ policy.status?.version ?? "Unknown" }}</UiKv>
          </UiKeyValue>
          <UiEmpty v-if="failed.token" dense title="The token could not be read.">
            <template #actions><EButton variant="quiet" size="sm" @click="load('token')">Try again</EButton></template>
          </UiEmpty>
          <UiField
            v-else
            v-slot="{ id }"
            label="Token"
            hint="Paste this token on chess.dame.gg to let it reach this machine. Anyone with it can use this server."
          >
            <div class="copy">
              <EPasswordField
                v-if="machineStore.token"
                :id="id"
                :model-value="machineStore.token"
                label="Token"
                hide-label
                readonly
                autocomplete="off"
                class="grow"
              />
              <span v-else class="skel grow" aria-label="Reading the token" />
              <ECopyButton
                v-if="machineStore.token"
                :text="machineStore.token"
                label="Copy the token"
                variant="outline"
              />
            </div>
          </UiField>
          <EForm v-if="remote" class="token" @submit.prevent="pair">
            <UiField v-slot="{ id, describedby, invalid }" label="Server" :error="urlError">
              <ETextField
                :id="id"
                v-model="serverUrl"
                label="Server"
                hide-label
                :placeholder="example"
                autocomplete="off"
                required
                :aria-describedby="describedby"
                :aria-invalid="invalid || undefined"
              />
            </UiField>
            <UiField
              v-slot="{ id }"
              label="Token"
              hint="Run dameless token on the machine with the server, and paste what it prints."
            >
              <EPasswordField :id="id" v-model="token" label="Token" hide-label required autocomplete="off" />
            </UiField>
            <div class="actions">
              <EButton
                type="submit"
                variant="outline"
                size="sm"
                :disabled="!serverUrl.trim() || !token.trim() || !!urlError"
              >
                Connect to it
              </EButton>
            </div>
          </EForm>
        </UiSection>
      </div>
    </template>
  </UiSheetPage>
</template>

<style scoped>
.muted {
  margin: 0;
  color: var(--c-ink-2);
  font: var(--t-meta);
}

.actions {
  display: flex;
  flex-wrap: wrap;
  align-items: center;
  gap: var(--sp-2);
}

/* A form in a section: its fields as wide as the section, its button under them. */
.token {
  display: grid;
  gap: var(--sp-3);
}

.block {
  block-size: var(--h-row-2);
}

/* The token's field stretches; its copy button sits level with the box. */
.copy {
  display: flex;
  align-items: flex-start;
  gap: var(--sp-2);
}

.grow {
  flex: 1 1 auto;
  min-width: 0;
}

/* Health: failures first, each with its words; a healthy one is a green dot and a line. */
.health {
  display: grid;
  margin: 0;
  padding: 0;
  list-style: none;
}

.item {
  display: grid;
  grid-template-columns: 5rem minmax(0, 1fr);
  align-items: start;
  gap: var(--sp-2);
  padding-block: var(--sp-2);
  border-block-end: var(--e-stroke-hair) solid var(--c-line);
}

.item:last-child {
  border-block-end: 0;
}

.item--ok {
  grid-template-columns: 1.25rem minmax(0, 1fr);
}

.mark {
  display: flex;
  align-items: center;
  min-block-size: var(--h-badge);
}

.what {
  display: grid;
  gap: var(--sp-1);
  min-width: 0;
}

.title {
  font: var(--t-body-strong);
}

.detail {
  color: var(--c-ink-2);
  font: var(--t-meta);
  overflow-wrap: anywhere;
}

.item--ok .what {
  display: block;
}

.item--ok .detail::before {
  content: " ";
}

.fix {
  display: grid;
  gap: var(--sp-1);
  margin-block-start: var(--sp-1);
}

.fix-label {
  font: var(--t-meta);
}

.fix-line {
  display: flex;
  align-items: center;
  gap: var(--sp-2);
  padding-inline-start: var(--sp-2);
  border-radius: var(--r-1);
  background: var(--c-raise);
}

.fix-command {
  flex: 1 1 auto;
  min-width: 0;
  font: var(--t-fig);
  overflow-wrap: anywhere;
}
</style>
