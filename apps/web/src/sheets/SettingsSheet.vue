<script setup lang="ts">
import { computed, onMounted, reactive, ref } from "vue";
import { useToast } from "@eos-ui/vue";
import { ESheet } from "@eos-ui/vue/components/ESheet";
import { EButton } from "@eos-ui/vue/components/EButton";
import { ECopyButton } from "@eos-ui/vue/components/ECopyButton";
import { EDescriptionList } from "@eos-ui/vue/components/EDescriptionList";
import { EForm } from "@eos-ui/vue/components/EForm";
import { ENotice } from "@eos-ui/vue/components/ENotice";
import { EPasswordField } from "@eos-ui/vue/components/EPasswordField";
import { ESkeleton } from "@eos-ui/vue/components/ESkeleton";
import { EStamp } from "@eos-ui/vue/components/EStamp";
import { ESegmented } from "@eos-ui/vue/components/ESegmented";
import { ETextField } from "@eos-ui/vue/components/ETextField";
import { connect, connection } from "../api/client";
import type { ConnectionKey, ConnectionStatus, HealthLevel, Pruned } from "../api/types";
import { bytes, plural, shortDate } from "../format";
import ConfirmDialog from "../parts/ConfirmDialog.vue";
import { choice, type Choice } from "../scheme";
import { useConnections } from "../stores/connections";
import { useDameless } from "../stores/dameless";
import { usePolicy } from "../stores/policy";
import SheetBack from "./SheetBack.vue";
import { useSheet } from "./useSheet";

/**
 * Settings: the two Lichess connections (your login, the bot's token), the machine (cores, the engine checkout, the
 * binaries kept and the disk they take), its health (what the work needs and whether it is there), whether the harness
 * can be trusted here, the theme, and the token the hosted app needs. Logging out and disconnecting ask first; so do
 * freeing disk space, after saying what it would free, and checking the harness, which plays games for a while. What
 * cannot be read says so, with a way to try again, rather than staying a placeholder.
 */
const sheet = useSheet();
const policy = usePolicy();
const connections = useConnections();
const dameless = useDameless();
const toast = useToast();

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
type Tone = "neutral" | "success" | "info" | "warning" | "danger";
const stateOf = (c: ConnectionStatus): { word: string; tone: Tone } =>
  c.state === "online"
    ? { word: "Connected", tone: "success" }
    : c.state === "connecting"
      ? { word: "Connecting", tone: "info" }
      : c.state === "error"
        ? { word: "Cannot reach Lichess", tone: "danger" }
        : { word: "Offline", tone: "neutral" };

/** Health, less what this sheet already shows in its own sections: the builds and the checkout (This machine) and the harness. */
const healthItems = computed(() =>
  (dameless.health?.items ?? []).filter((i) => !["builds", "harness", "engine"].includes(i.key)),
);

/** A linked account's rows: who, in what state, and for your login how long it lasts. A missing link has no rows. */
const lichessItems = (c: ConnectionStatus) => {
  if (!c.configured) return [];
  const s = stateOf(c);
  return [
    {
      label: "Account",
      text: c.account ? `${c.account.username}${c.account.title ? ` (${c.account.title})` : ""}` : "Linked",
      stamp: s.word,
      stampTone: s.tone,
    },
    ...(c.state === "error" && c.error ? [{ label: "Problem", text: c.error }] : []),
    ...(c.key === "you" && c.you?.expires_at
      ? [
          {
            label: "Login ends",
            text: shortDate(c.you.expires_at),
          },
        ]
      : []),
  ];
};

const loggingIn = ref(false);
async function login() {
  loggingIn.value = true;
  const r = await attempt("Login did not start", () => connections.login());
  if (r) window.location.assign(r.url);
  else loggingIn.value = false;
}

const leaving = ref<ConnectionKey | null>(null);
const leaveBusy = ref(false);
const leaveOpen = computed({
  get: () => leaving.value !== null,
  set: (v: boolean) => {
    if (!v) leaving.value = null;
  },
});
async function leave() {
  const key = leaving.value;
  if (!key) return;
  leaveBusy.value = true;
  const done = await attempt(key === "you" ? "Logging out did not work" : "Disconnecting did not work", () =>
    connections.disconnect(key),
  );
  leaveBusy.value = false;
  if (done !== undefined || connections[key].configured === false) leaving.value = null;
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
const machineItems = computed(() => {
  const m = machine.value;
  return [
    { label: "Cores", text: policy.status ? String(policy.status.cores) : "Unknown" },
    {
      label: "Engine checkout",
      text: m?.engine
        ? `${m.engine.dir}${m.engine.branch ? ` · ${m.engine.branch}` : ""}${m.engine.commit ? ` at ${m.engine.commit.slice(0, 8)}` : ""}${m.engine.dirty ? " · has uncommitted changes" : ""}`
        : "Not found",
    },
    { label: "Data", text: m?.data_dir ?? "Unknown" },
    {
      label: "Binaries",
      // The bytes include the build cache, so they are not zero without binaries: say which is which.
      text: m
        ? m.builds.count
          ? `${plural(m.builds.count, "binary", "binaries")}, ${bytes(m.builds.bytes)} with the build cache`
          : `None yet (${bytes(m.builds.bytes)} of build cache)`
        : "Unknown",
    },
  ];
});

// ---- Health and the harness

const LEVEL: Record<HealthLevel, { tone: "success" | "info" | "warning" | "danger"; word: string }> = {
  ok: { tone: "success", word: "Ready" },
  info: { tone: "info", word: "Note" },
  warn: { tone: "warning", word: "Check" },
  fail: { tone: "danger", word: "Missing" },
};
/** fastchess or the book is missing: the one thing Install tools fixes. */
const toolsMissing = computed(() =>
  (dameless.health?.items ?? []).some((i) => (i.key === "fastchess" || i.key === "book") && i.level === "fail"),
);
const installing = ref(false);
const installBusy = ref(false);
async function installTools() {
  installBusy.value = true;
  const r = await attempt("The tools were not installed", () => dameless.installTools());
  installBusy.value = false;
  if (!r) return;
  installing.value = false;
  toast.show({ title: "The tools are installed", tone: "success" });
}
const harness = computed(() => dameless.harness);
const harnessStamp = computed(() => {
  const h = harness.value;
  if (!h) return null;
  if (h.running != null) return { word: "Checking", tone: "info" as const };
  return h.trusted
    ? { word: "Trusted", tone: "success" as const }
    : { word: "Not trusted here", tone: "warning" as const };
});
const checking = ref(false);
const checkBusy = ref(false);
async function checkHarness() {
  checkBusy.value = true;
  const idea = await attempt("The check did not start", () => dameless.checkHarness());
  checkBusy.value = false;
  if (!idea) return;
  checking.value = false;
  toast.show({ title: "Checking the harness", message: "The status line shows how it goes.", tone: "info" });
}

const pruning = ref(false);
const plan = ref<Pruned | null>(null);
const pruneBusy = ref(false);
async function planPrune() {
  pruneBusy.value = true;
  const r = await attempt("The check did not work", () => dameless.prune(true));
  pruneBusy.value = false;
  if (!r) return;
  if (r.builds.length === 0) {
    toast.show({ title: "Nothing to free", message: "Every binary kept is the newest of its version.", tone: "info" });
    return;
  }
  plan.value = r;
  pruning.value = true;
}
async function prune() {
  pruneBusy.value = true;
  const r = await attempt("The disk was not freed", () => dameless.prune(false));
  pruneBusy.value = false;
  if (r) {
    pruning.value = false;
    toast.show({
      title: `Freed ${bytes(r.bytes)}`,
      message: `${plural(r.builds.length, "old binary", "old binaries")} removed.`,
      tone: "success",
    });
  }
}

// ---- This page and its server

const facts = computed(() => [
  { label: "Server", text: connection.url || location.origin },
  { label: "Version", text: policy.status?.version ?? "Unknown" },
]);

/** Only a page from another origin (the hosted site) pairs with a server; one the server serves needs nothing. */
const remote = computed(() => connection.url !== "");
const url = ref(connection.url);
const token = ref("");
const urlError = computed(() =>
  url.value && !/^https?:\/\/[^\s/]+\S*$/.test(url.value.trim())
    ? "Start with http:// or https://, as in http://127.0.0.1:7979."
    : undefined,
);
function pair() {
  connect(url.value.trim(), token.value.trim());
  location.reload();
}
</script>

<template>
  <ESheet v-bind="sheet.bind" title="Settings" description="Lichess, the machine, the theme and the hosted app.">
    <template #leading><SheetBack :show="sheet.canGoBack" @back="sheet.back" /></template>
    <section class="sec" aria-labelledby="s-you">
      <h2 id="s-you" class="part-title">Your Lichess account</h2>
      <ESkeleton v-if="!connections.loaded" :lines="2" label="Reading your Lichess account" />
      <template v-else>
        <EDescriptionList v-if="you.configured" :items="lichessItems(you)" />
        <p v-else class="sec-note">Not linked. Log in to play on Lichess from here.</p>
        <ENotice v-if="you.you?.missing_scopes.length" tone="warning" title="The login is missing a permission">
          It lacks {{ you.you.missing_scopes.join(", ") }}. Log in again to give it.
        </ENotice>
        <div class="sheet-actions">
          <EButton v-if="!you.configured" :loading="loggingIn" @click="login">Log in with Lichess</EButton>
          <EButton v-else variant="outline" @click="leaving = 'you'">Log out</EButton>
        </div>
      </template>
    </section>

    <section class="sec" aria-labelledby="s-bot">
      <h2 id="s-bot" class="part-title">Dameless's BOT account</h2>
      <ESkeleton v-if="!connections.loaded" :lines="2" label="Reading the bot's account" />
      <template v-else>
        <EDescriptionList v-if="bot.configured" :items="lichessItems(bot)" />
        <EForm v-if="!bot.configured" class="token" @submit.prevent="saveBotToken">
          <EPasswordField
            v-model="botToken"
            label="Personal token"
            autocomplete="off"
            required
            :error="tokenError"
            hint="Made on lichess.org with the bot:play scope, for a BOT account. Kept on this machine, never shown again."
            @update:model-value="tokenError = undefined"
          />
          <div class="sheet-actions">
            <EButton type="submit" :loading="savingToken" :disabled="botToken.trim().length < 8">Connect</EButton>
          </div>
        </EForm>
        <div v-else class="sheet-actions">
          <EButton variant="outline" @click="leaving = 'bot'">Disconnect</EButton>
        </div>
      </template>
    </section>

    <section class="sec" aria-labelledby="s-machine">
      <h2 id="s-machine" class="part-title">This machine</h2>
      <ENotice v-if="failed.machine" tone="warning" title="The machine could not be read">
        Dameless did not answer.
        <template #actions><EButton variant="outline" size="sm" @click="load('machine')">Try again</EButton></template>
      </ENotice>
      <ESkeleton v-else-if="!machine" :lines="4" label="Reading the machine" />
      <template v-else>
        <EDescriptionList :items="machineItems" />
        <div class="sheet-actions">
          <EButton
            variant="outline"
            :loading="pruneBusy && !pruning"
            :disabled="!machine.builds.count"
            @click="planPrune"
          >
            Free disk space
          </EButton>
        </div>
        <p class="sec-note">Old engine binaries are rebuilt when needed. The newest of each version is kept.</p>
      </template>
    </section>

    <section class="sec" aria-labelledby="s-health">
      <h2 id="s-health" class="part-title">Health</h2>
      <ENotice v-if="failed.health" tone="warning" title="The health check did not run">
        Dameless did not answer.
        <template #actions><EButton variant="outline" size="sm" @click="load('health')">Try again</EButton></template>
      </ENotice>
      <ESkeleton v-else-if="!dameless.health" :lines="3" label="Checking the machine" />
      <ul v-else class="plain rows">
        <li v-for="item in healthItems" :key="item.key" class="row">
          <div class="row-head">
            <p class="row-title">{{ item.title }}</p>
            <EStamp size="sm" :tone="LEVEL[item.level].tone" :label="LEVEL[item.level].word" />
          </div>
          <p class="sec-note wrap">{{ item.detail }}</p>
          <p v-if="item.action" class="sec-note fix-label">{{ item.action.label }}</p>
          <div v-if="item.action" class="fix">
            <code class="mono fix-command">{{ item.action.command }}</code>
            <ECopyButton :text="item.action.command" :label="`Copy the command for ${item.title}`" size="sm" />
          </div>
        </li>
      </ul>
      <div v-if="toolsMissing" class="sheet-actions">
        <EButton variant="outline" @click="installing = true">Install tools</EButton>
      </div>
    </section>

    <section class="sec" aria-labelledby="s-harness">
      <h2 id="s-harness" class="part-title">The harness</h2>
      <ENotice v-if="failed.harness" tone="warning" title="The last check could not be read">
        Dameless did not answer.
        <template #actions><EButton variant="outline" size="sm" @click="load('harness')">Try again</EButton></template>
      </ENotice>
      <ESkeleton v-else-if="!harness || !harnessStamp" :lines="2" label="Reading the last check" />
      <template v-else>
        <div class="row">
          <div class="row-head">
            <p class="row-title">Last check</p>
            <EStamp size="sm" :tone="harnessStamp.tone" :label="harnessStamp.word" />
          </div>
          <p class="sec-note wrap">{{ harness.summary }}</p>
        </div>
        <div class="sheet-actions">
          <EButton
            variant="outline"
            :disabled="harness.running != null"
            :loading="checkBusy && !checking"
            @click="checking = true"
          >
            Check the harness
          </EButton>
        </div>
        <p class="sec-note">
          Plays the champion against an identical copy. A change that does nothing should not pass; if it does, the
          results on this machine are not to be trusted.
        </p>
      </template>
    </section>

    <section class="sec" aria-labelledby="s-look">
      <h2 id="s-look" class="part-title">Appearance</h2>
      <ESegmented v-model="choice" label="Theme" hide-label :options="themes" />
    </section>

    <section class="sec" aria-labelledby="s-token">
      <h2 id="s-token" class="part-title">The hosted app</h2>
      <EDescriptionList :items="facts" />
      <ENotice v-if="failed.token" tone="warning" title="The token could not be read">
        Dameless did not answer.
        <template #actions><EButton variant="outline" size="sm" @click="load('token')">Try again</EButton></template>
      </ENotice>
      <div v-else-if="dameless.token" class="copy">
        <EPasswordField :model-value="dameless.token" label="Token" readonly autocomplete="off" class="grow" />
        <ECopyButton :text="dameless.token" label="Copy the token" variant="outline" />
      </div>
      <ESkeleton v-else :lines="1" label="Reading the token" />
      <p v-if="!failed.token" class="sec-note">
        Paste this token on chess.dame.gg to let it reach this machine. Anyone with it can use this server.
      </p>
      <EForm v-if="remote" class="token" @submit.prevent="pair">
        <ETextField
          v-model="url"
          label="Server"
          placeholder="http://127.0.0.1:7979"
          autocomplete="off"
          required
          :error="urlError"
        />
        <EPasswordField
          v-model="token"
          label="Token"
          required
          autocomplete="off"
          hint="Run dameless token on the machine with the server, and paste what it prints."
        />
        <div class="sheet-actions">
          <EButton type="submit" variant="outline" :disabled="!url.trim() || !token.trim() || !!urlError">
            Connect
          </EButton>
        </div>
      </EForm>
    </section>

    <ConfirmDialog
      v-model:open="leaveOpen"
      :title="leaving === 'you' ? 'Log out of Lichess?' : 'Disconnect the bot?'"
      :description="
        leaving === 'you'
          ? 'Your login is revoked at Lichess and forgotten here.'
          : 'Dameless goes offline and its token is forgotten.'
      "
      :confirm="leaving === 'you' ? 'Log out' : 'Disconnect'"
      danger
      :busy="leaveBusy"
      @confirm="leave"
    >
      <p class="text">
        {{
          leaving === "you"
            ? "Games you are playing on Lichess carry on there. You can log in again at any time."
            : "Games it is playing finish first. You will need a token to connect it again."
        }}
      </p>
    </ConfirmDialog>

    <ConfirmDialog
      v-model:open="installing"
      title="Install the tools?"
      description="Downloads fastchess and the opening book, which trials need."
      confirm="Install"
      :busy="installBusy"
      @confirm="installTools"
    >
      <p class="text">
        fastchess v1.8.2 comes from its GitHub release (github.com/Disservin/fastchess), and the UHO opening book from
        the Stockfish books repository (github.com/official-stockfish/books). Together a few tens of megabytes; they go
        into the tools folder in the data directory. This can take a minute.
      </p>
    </ConfirmDialog>

    <ConfirmDialog
      v-model:open="checking"
      title="Check the harness?"
      description="The champion plays an identical copy of itself: 1,000 pairs at fixed nodes, where every pair must be the same game twice, then 4,000 pairs of quick timed games, which must come out even and symmetric."
      confirm="Start the check"
      :busy="checkBusy"
      @confirm="checkHarness"
    >
      <p class="text">
        It takes the machine's cores for a few hours. Other work waits its turn, and live games still come first.
      </p>
    </ConfirmDialog>

    <ConfirmDialog
      v-model:open="pruning"
      title="Free disk space?"
      :description="
        plan ? `${plural(plan.builds.length, 'old binary', 'old binaries')}, about ${bytes(plan.bytes)}.` : undefined
      "
      :confirm="plan ? `Free ${bytes(plan.bytes)}` : 'Free'"
      :busy="pruneBusy"
      @confirm="prune"
    >
      <p class="text">Only binaries that no trial, tune, game or the bot uses are removed. Nothing else is touched.</p>
    </ConfirmDialog>
  </ESheet>
</template>

<style scoped>
/* A form in a section: its fields as wide as the section, its button under them. */
.token {
  display: grid;
  gap: var(--e-gap-snug);
}

/* Long paths and addresses break where they must instead of widening the sheet. */
.sec :deep(.e-fact-value) {
  overflow-wrap: anywhere;
}

/* The token's field stretches; its copy button sits level with the box (the field has no hint under it). */
.copy {
  display: flex;
  align-items: flex-end;
  gap: var(--e-gap-snug);
}

.grow {
  flex: 1 1 auto;
  min-width: 0;
}

.text {
  margin: 0;
}

/* Health and the harness: a title with its stamp at the end, the detail under it, ruled apart. */
.rows {
  gap: 0;
}

.row {
  display: grid;
  gap: 2px;
  min-width: 0;
  padding-block: var(--e-gap-snug);
}

.rows .row + .row {
  border-block-start: var(--line);
}

.rows .row:first-child {
  padding-block-start: 0;
}

.row-head {
  display: flex;
  align-items: center;
  justify-content: space-between;
  gap: var(--e-gap-snug);
}

.row-title {
  min-width: 0;
  margin: 0;
  font-weight: 600;
}

.wrap {
  overflow-wrap: anywhere;
}

/* The command that fixes a missing piece, in a block with a button that copies it. */
.fix-label {
  margin-block-start: 0.25rem;
}

.fix {
  display: flex;
  align-items: center;
  gap: var(--e-gap-snug);
  padding-inline-start: var(--e-gap-snug);
  border: var(--line);
  border-radius: var(--e-radius-sm);
  background: color-mix(in oklab, var(--e-muted) 8%, transparent);
}

.fix-command {
  flex: 1 1 auto;
  min-width: 0;
  overflow-wrap: anywhere;
  font-size: 0.8125rem;
}
</style>
