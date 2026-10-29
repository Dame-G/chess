<script setup lang="ts">
import { computed, onMounted, reactive, ref } from "vue";
import { RouterLink } from "vue-router";
import { useToast } from "@eos-ui/vue";
import { EButton } from "@eos-ui/vue/components/EButton";
import { ECopyButton } from "@eos-ui/vue/components/ECopyButton";
import { EForm } from "@eos-ui/vue/components/EForm";
import { ELink } from "@eos-ui/vue/components/ELink";
import { ENotice } from "@eos-ui/vue/components/ENotice";
import { EPasswordField } from "@eos-ui/vue/components/EPasswordField";
import { ETextField } from "@eos-ui/vue/components/ETextField";
import { connect, connection } from "../api/client";
import type { ConnectionKey, ConnectionStatus, HealthItem } from "../api/types";
import { bytes, formatDate, plural } from "../format";
import PathText from "../parts/PathText.vue";
import { choice, type Choice } from "../scheme";
import { useConnections } from "../stores/connections";
import { useDameless } from "../stores/dameless";
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
 * Settings: the two Lichess connections (your login, the bot's token), the machine (cores, the engine checkout, the
 * binaries kept and the disk they take, and whether the harness can be trusted here), its health (what the work needs
 * and whether it is there, failures first), and how the app looks, with the token the hosted app needs. Logging out and
 * disconnecting ask first; so do freeing disk space, after saying what it would free, and checking the harness, which
 * plays games for a while. What cannot be read says so, with a way to try again, rather than staying a placeholder.
 */
const policy = usePolicy();
const connections = useConnections();
const dameless = useDameless();
const toast = useToast();
const confirm = useConfirm();

const tab = ref("accounts");
const themes: { value: Choice; label: string }[] = [
  { value: "system", label: "System" },
  { value: "light", label: "Light" },
  { value: "dark", label: "Dark" },
];

/** What could not be read, by part, so each says so and can be tried again on its own. */
const failed = reactive({ machine: false, health: false, harness: false, token: false });
const loaders = {
  machine: () => dameless.loadMachine(),
  health: () => dameless.loadHealth(),
  harness: () => dameless.loadHarness(),
  token: () => dameless.loadToken(),
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
const bot = computed(() => connections.bot);
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

async function leave(key: ConnectionKey) {
  const you = key === "you";
  const yes = await confirm({
    title: you ? "Log out of Lichess?" : "Disconnect the bot?",
    body: you
      ? "Your login is revoked at Lichess and forgotten here. Games you are playing there carry on. You can log in again at any time."
      : "Dameless goes offline and its token is forgotten. Games it is playing finish first. You will need a token to connect it again.",
    confirmLabel: you ? "Log out" : "Disconnect the bot",
    danger: true,
  });
  if (!yes) return;
  await attempt(you ? "Logging out did not work" : "Disconnecting did not work", () => connections.disconnect(key));
}

const botToken = ref("");
const tokenError = ref<string | undefined>(undefined);
const savingToken = ref(false);
async function saveBotToken() {
  savingToken.value = true;
  tokenError.value = undefined;
  try {
    await connections.setToken("bot", botToken.value.trim());
    botToken.value = "";
    toast.show({ title: "The bot is connected", tone: "success" });
  } catch (e) {
    tokenError.value = e instanceof Error ? e.message : "Lichess did not accept that token.";
  } finally {
    savingToken.value = false;
  }
}

// ---- Machine

const machine = computed(() => dameless.machine);
const engine = computed(() => machine.value?.engine);
const binaries = computed(() => {
  const m = machine.value;
  if (!m) return "Unknown";
  // The bytes include the build cache, so they are not zero without binaries: say which is which.
  return m.builds.count
    ? `${plural(m.builds.count, "binary", "binaries")}, ${bytes(m.builds.bytes)} with the cache`
    : `None yet (${bytes(m.builds.bytes)} of cache)`;
});

const harness = computed(() => dameless.harness);
const harnessBadge = computed(() => {
  const h = harness.value;
  if (!h) return null;
  if (h.running != null) return { word: "Checking", tone: "info" as const };
  return h.trusted ? { word: "Trusted", tone: "win" as const } : { word: "Not trusted here", tone: "warn" as const };
});
const checkBusy = ref(false);
async function checkHarness() {
  const yes = await confirm({
    title: "Check the harness?",
    body: "The champion plays an identical copy of itself: 1,000 pairs at fixed nodes, where every pair must be the same game twice, then 4,000 pairs of quick timed games, which must come out even and symmetric. It takes the machine's cores for a few hours; other work waits its turn, and live games still come first.",
    confirmLabel: "Start the check",
  });
  if (!yes) return;
  checkBusy.value = true;
  const idea = await attempt("The check did not start", () => dameless.checkHarness());
  checkBusy.value = false;
  if (idea) toast.show({ title: "Checking the harness", message: "The header shows how it goes.", tone: "info" });
}

const pruneBusy = ref(false);
async function freeDisk() {
  pruneBusy.value = true;
  const plan = await attempt("The check did not work", () => dameless.prune(true));
  pruneBusy.value = false;
  if (!plan) return;
  if (plan.builds.length === 0) {
    toast.show({ title: "Nothing to free", message: "Every binary kept is the newest of its version.", tone: "info" });
    return;
  }
  const yes = await confirm({
    title: `Free ${bytes(plan.bytes)}?`,
    body: `${plural(plan.builds.length, "old binary", "old binaries")} would be removed. Only binaries that no trial, tune, game or the bot uses go; nothing else is touched, and they are rebuilt when needed.`,
    confirmLabel: `Free ${bytes(plan.bytes)}`,
  });
  if (!yes) return;
  pruneBusy.value = true;
  const r = await attempt("The disk was not freed", () => dameless.prune(false));
  pruneBusy.value = false;
  if (r) {
    toast.show({
      title: `Freed ${bytes(r.bytes)}`,
      message: `${plural(r.builds.length, "old binary", "old binaries")} removed.`,
      tone: "success",
    });
  }
}

// ---- Health

/** Health, less what the machine tab already shows: the builds, the checkout and the harness. Failures first. */
const ORDER = { fail: 0, warn: 1, info: 2, ok: 3 } as const;
const healthItems = computed(() =>
  (dameless.health?.items ?? [])
    .filter((i) => !["builds", "harness", "engine"].includes(i.key))
    .sort((a, b) => ORDER[a.level] - ORDER[b.level]),
);
const problems = computed(() => healthItems.value.filter((i) => i.level !== "ok").length);
const isOk = (i: HealthItem) => i.level === "ok";
/** The detail without the title it starts with: "fastchess 1.4.0 is installed." under "fastchess" says "1.4.0 is installed.". */
const detailOf = (i: HealthItem) =>
  i.detail.toLowerCase().startsWith(i.title.toLowerCase()) ? i.detail.slice(i.title.length).trim() : i.detail;
/** fastchess or the book is missing: the one thing Install tools fixes. */
const toolsMissing = computed(() =>
  (dameless.health?.items ?? []).some((i) => (i.key === "fastchess" || i.key === "book") && i.level === "fail"),
);
const installBusy = ref(false);
async function installTools() {
  const yes = await confirm({
    title: "Install the tools?",
    body: "Downloads fastchess (from its GitHub release, Disservin/fastchess) and the UHO opening book (official-stockfish/books), which trials need. Together a few tens of megabytes, into the tools folder in the data directory. This can take a minute.",
    confirmLabel: "Install",
  });
  if (!yes) return;
  installBusy.value = true;
  const r = await attempt("The tools were not installed", () => dameless.installTools());
  installBusy.value = false;
  if (r) toast.show({ title: "The tools are installed", tone: "success" });
}

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
            <div class="actions"><EButton variant="outline" size="sm" @click="leave('you')">Log out</EButton></div>
          </template>
          <UiEmpty v-else dense title="Not connected." body="Log in to play on Lichess from here.">
            <template #actions>
              <EButton size="sm" :loading="loggingIn" @click="login">Log in with Lichess</EButton>
            </template>
          </UiEmpty>
        </UiSection>

        <UiSection title="The bot">
          <template v-if="connections.loaded && bot.configured" #aside>
            <UiBadge :tone="stateOf(bot).tone" dot>{{ stateOf(bot).word }}</UiBadge>
          </template>
          <span v-if="!connections.loaded" class="skel block" aria-label="Reading the bot's account" />
          <template v-else-if="bot.configured">
            <UiKeyValue dense>
              <UiKv label="Account">{{ accountName(bot) }}</UiKv>
              <UiKv v-if="bot.state === 'error' && bot.error" label="Problem" tone="loss">{{ bot.error }}</UiKv>
              <UiKv label="Controls">
                <ELink :link-component="RouterLink" :to="{ path: '/dameless', hash: '#bot' }">Bot controls</ELink>
              </UiKv>
            </UiKeyValue>
            <div class="actions">
              <EButton variant="outline" size="sm" @click="leave('bot')">Disconnect the bot</EButton>
            </div>
          </template>
          <EForm v-else class="token" @submit.prevent="saveBotToken">
            <UiField
              v-slot="{ id, describedby, invalid }"
              label="Personal token"
              :error="tokenError"
              hint="Made on lichess.org with the bot:play scope, for a BOT account. Kept on this machine, never shown again."
            >
              <EPasswordField
                :id="id"
                v-model="botToken"
                label="Personal token"
                hide-label
                autocomplete="off"
                required
                :aria-describedby="describedby"
                :aria-invalid="invalid || undefined"
                @update:model-value="tokenError = undefined"
              />
            </UiField>
            <div class="actions">
              <EButton type="submit" size="sm" :loading="savingToken" :disabled="botToken.trim().length < 8">
                Connect the bot
              </EButton>
            </div>
          </EForm>
        </UiSection>
      </div>
    </template>

    <template #tab-machine>
      <div>
        <UiSection title="Engine and data">
          <UiEmpty v-if="failed.machine" dense title="The machine could not be read.">
            <template #actions>
              <EButton variant="quiet" size="sm" @click="load('machine')">Try again</EButton>
            </template>
          </UiEmpty>
          <span v-else-if="!machine" class="skel block" aria-label="Reading the machine" />
          <UiKeyValue v-else dense>
            <UiKv label="Cores">{{ policy.status ? policy.status.cores : "Unknown" }}</UiKv>
            <UiKv label="Engine checkout" copy>
              <PathText v-if="engine?.dir" :text="engine.dir" />
              <template v-else>Not found</template>
            </UiKv>
            <UiKv v-if="engine?.dir && (engine.branch || engine.commit)" label="Checked out" mono>
              {{ engine.branch ?? "detached"
              }}<template v-if="engine.commit"> at {{ engine.commit.slice(0, 8) }}</template>
              <UiBadge v-if="engine.dirty" tone="warn">Uncommitted changes</UiBadge>
            </UiKv>
            <UiKv label="Data" copy><PathText :text="machine.data_dir" /></UiKv>
            <UiKv label="Binaries">
              {{ binaries }}
              <EButton
                variant="quiet"
                size="sm"
                :loading="pruneBusy"
                :disabled="!machine.builds.count"
                title="Old engine binaries are rebuilt when needed. The newest of each version is kept."
                @click="freeDisk"
              >
                Free disk space
              </EButton>
            </UiKv>
          </UiKeyValue>
        </UiSection>

        <UiSection title="Harness">
          <UiEmpty v-if="failed.harness" dense title="The last check could not be read.">
            <template #actions>
              <EButton variant="quiet" size="sm" @click="load('harness')">Try again</EButton>
            </template>
          </UiEmpty>
          <span v-else-if="!harness || !harnessBadge" class="skel block" aria-label="Reading the last check" />
          <template v-else>
            <UiKeyValue dense>
              <UiKv label="Status">
                <UiBadge :tone="harnessBadge.tone">{{ harnessBadge.word }}</UiBadge>
              </UiKv>
              <UiKv label="Last check">{{ harness.summary }}</UiKv>
            </UiKeyValue>
            <div class="actions">
              <EButton
                variant="outline"
                size="sm"
                title="Plays the champion against an identical copy. A change that does nothing should not pass; if it does, results on this machine are not to be trusted."
                :disabled="harness.running != null"
                :loading="checkBusy"
                @click="checkHarness"
              >
                Check the harness
              </EButton>
            </div>
          </template>
        </UiSection>
      </div>
    </template>

    <template #tab-health>
      <div>
        <UiSection title="Checks">
          <template v-if="toolsMissing" #aside>
            <EButton variant="outline" size="sm" :loading="installBusy" @click="installTools">Install tools</EButton>
          </template>
          <UiEmpty v-if="failed.health" dense title="The health check did not run.">
            <template #actions><EButton variant="quiet" size="sm" @click="load('health')">Try again</EButton></template>
          </UiEmpty>
          <span v-else-if="!dameless.health" class="skel block" aria-label="Checking the machine" />
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
                v-if="dameless.token"
                :id="id"
                :model-value="dameless.token"
                label="Token"
                hide-label
                readonly
                autocomplete="off"
                class="grow"
              />
              <span v-else class="skel grow" aria-label="Reading the token" />
              <ECopyButton v-if="dameless.token" :text="dameless.token" label="Copy the token" variant="outline" />
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
