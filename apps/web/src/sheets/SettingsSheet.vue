<script setup lang="ts">
import { computed, onMounted, ref } from "vue";
import { useToast } from "@eos-ui/vue";
import { ESheet } from "@eos-ui/vue/components/ESheet";
import { EActions } from "@eos-ui/vue/components/EActions";
import { EButton } from "@eos-ui/vue/components/EButton";
import { ECopyButton } from "@eos-ui/vue/components/ECopyButton";
import { EDescriptionList } from "@eos-ui/vue/components/EDescriptionList";
import { ESkeleton } from "@eos-ui/vue/components/ESkeleton";
import { EStamp } from "@eos-ui/vue/components/EStamp";
import { ESegmented } from "@eos-ui/vue/components/ESegmented";
import { EStack } from "@eos-ui/vue/components/EStack";
import { ETextField } from "@eos-ui/vue/components/ETextField";
import { connect, connection } from "../api/client";
import type { ConnectionKey, HealthLevel, Pruned } from "../api/types";
import { bytes, plural } from "../format";
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
 * freeing disk space, after saying what it would free, and checking the harness, which plays games for a while.
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

onMounted(() => {
  void dameless.loadMachine().catch(() => {});
  void dameless.loadToken().catch(() => {});
  void dameless.loadHealth().catch(() => {});
  void dameless.loadHarness().catch(() => {});
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
const stateWord = (c: { configured: boolean; state: string; error?: string | null }) =>
  !c.configured
    ? "Not linked"
    : c.state === "online"
      ? "Connected"
      : c.state === "connecting"
        ? "Connecting…"
        : c.state === "error"
          ? (c.error ?? "Cannot reach Lichess")
          : "Linked, offline";
/** Health, less what this sheet already shows in its own sections: the builds (This machine) and the harness. */
const healthItems = computed(() =>
  (dameless.health?.items ?? []).filter((i) => i.key !== "builds" && i.key !== "harness"),
);

const lichessItems = (c: typeof you.value) => [
  ...(c.account
    ? [{ label: "Account", text: `${c.account.username}${c.account.title ? ` (${c.account.title})` : ""}` }]
    : []),
  { label: "Status", text: stateWord(c) },
  ...(c.key === "you" && c.you?.expires_at
    ? [
        {
          label: "Login lasts until",
          text: new Date(c.you.expires_at).toLocaleDateString("en-GB", {
            day: "numeric",
            month: "long",
            year: "numeric",
          }),
        },
      ]
    : []),
];

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
const savingToken = ref(false);
const tokenReady = computed(() => botToken.value.trim().length >= 8);
async function saveBotToken() {
  savingToken.value = true;
  const ok = await attempt("Lichess did not accept that token", () =>
    connections.setToken("bot", botToken.value.trim()),
  );
  savingToken.value = false;
  if (ok !== undefined || connections.bot.configured) {
    botToken.value = "";
    toast.show({ title: "The bot is connected", tone: "success" });
  }
}

// ---- Machine

const machine = computed(() => dameless.machine);
const machineItems = computed(() => {
  const m = machine.value;
  return [
    { label: "Cores for work", text: policy.status ? String(policy.status.cores) : "Unknown" },
    {
      label: "Engine checkout",
      text: m?.engine
        ? `${m.engine.dir}${m.engine.branch ? ` · ${m.engine.branch}` : ""}${m.engine.commit ? ` at ${m.engine.commit.slice(0, 8)}` : ""}${m.engine.dirty ? " · has uncommitted changes" : ""}`
        : "Not found",
    },
    { label: "Data", text: m?.data_dir ?? "Unknown" },
    {
      label: "Engine binaries kept",
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
  { label: "Server", text: connection.url || "The server this page came from" },
  { label: "Version", text: policy.status?.version ?? "Unknown" },
]);

/** Only a page from another origin (the hosted site) pairs with a server; one the server serves needs nothing. */
const remote = computed(() => connection.url !== "");
const url = ref(connection.url);
const token = ref("");
function pair() {
  connect(url.value, token.value);
  location.reload();
}
</script>

<template>
  <ESheet
    v-bind="sheet.bind"
    title="Settings"
    description="Lichess connections, the machine, the theme and the token for the hosted app."
  >
    <template #leading><SheetBack :show="sheet.canGoBack" @back="sheet.back" /></template>
    <section class="sec" aria-labelledby="s-you">
      <h2 id="s-you" class="part-title">Your Lichess account</h2>
      <EDescriptionList :items="lichessItems(you)" />
      <p v-if="you.you?.missing_scopes.length" class="sec-note">
        The login lacks {{ you.you.missing_scopes.join(", ") }}. Log in again to give it.
      </p>
      <div class="sheet-actions">
        <EButton v-if="!you.configured" :loading="loggingIn" @click="login">Log in with Lichess</EButton>
        <EButton v-else variant="outline" @click="leaving = 'you'">Log out</EButton>
      </div>
    </section>

    <section class="sec" aria-labelledby="s-bot">
      <h2 id="s-bot" class="part-title">Dameless's BOT account</h2>
      <EDescriptionList :items="lichessItems(bot)" />
      <form v-if="!bot.configured" class="token" @submit.prevent="saveBotToken">
        <ETextField
          v-model="botToken"
          type="password"
          label="Personal token"
          autocomplete="off"
          hint="Made on lichess.org with the bot:play scope, for an account that is already a BOT account. It is kept on this machine and never shown again."
        />
        <EButton type="submit" variant="outline" :loading="savingToken" :disabled="!tokenReady">Connect</EButton>
      </form>
      <div v-else class="sheet-actions">
        <EButton variant="outline" @click="leaving = 'bot'">Disconnect</EButton>
      </div>
    </section>

    <section class="sec" aria-labelledby="s-machine">
      <h2 id="s-machine" class="part-title">This machine</h2>
      <EDescriptionList :items="machineItems" />
      <div class="sheet-actions">
        <EButton
          variant="outline"
          :loading="pruneBusy && !pruning"
          :disabled="!machine?.builds.count"
          @click="planPrune"
        >
          Free disk space
        </EButton>
      </div>
      <p class="sec-note">Old engine binaries are rebuilt when needed. The newest of each version is kept.</p>
    </section>

    <section class="sec" aria-labelledby="s-health">
      <h2 id="s-health" class="part-title">Health</h2>
      <ESkeleton v-if="!dameless.health" :lines="3" label="Checking the machine" />
      <ul v-else class="plain health">
        <li v-for="item in healthItems" :key="item.key" class="health-item">
          <EStamp size="sm" :tone="LEVEL[item.level].tone" :label="LEVEL[item.level].word" />
          <div class="health-text">
            <p class="health-title">{{ item.title }}</p>
            <p class="sec-note">{{ item.detail }}</p>
            <p v-if="item.action" class="sec-note">
              {{ item.action.label }}: <code class="mono">{{ item.action.command }}</code>
            </p>
          </div>
        </li>
      </ul>
      <div v-if="toolsMissing" class="sheet-actions">
        <EButton variant="outline" @click="installing = true">Install tools</EButton>
      </div>
    </section>

    <section class="sec" aria-labelledby="s-harness">
      <h2 id="s-harness" class="part-title">The harness</h2>
      <ESkeleton v-if="!harness" :lines="2" label="Reading the last check" />
      <template v-else>
        <p class="harness-line">
          <EStamp
            size="sm"
            :tone="harness.running != null ? 'info' : harness.trusted ? 'success' : 'warning'"
            :label="harness.running != null ? 'Checking' : harness.trusted ? 'Trusted' : 'Not checked here'"
          />
          <span>{{ harness.summary }}</span>
        </p>
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
      <ESegmented v-model="choice" label="Theme" :options="themes" />
    </section>

    <section class="sec" aria-labelledby="s-token">
      <h2 id="s-token" class="part-title">The hosted app</h2>
      <EDescriptionList :items="facts" />
      <div v-if="dameless.token" class="copy">
        <ETextField
          :model-value="dameless.token"
          type="password"
          label="Token for the hosted app"
          readonly
          autocomplete="off"
          class="grow"
        />
        <ECopyButton :text="dameless.token" label="Copy the token" variant="outline" />
      </div>
      <p class="sec-note">
        Paste this token on chess.dame.gg to let it reach this machine. Anyone with it can use this server.
      </p>
      <EStack v-if="remote" gap="base">
        <ETextField v-model="url" label="Server" placeholder="http://127.0.0.1:7979" autocomplete="off" />
        <ETextField
          v-model="token"
          label="Token"
          hint="Run dameless token on the machine with the server, and paste what it prints."
          autocomplete="off"
        />
        <EActions>
          <EButton variant="outline" :disabled="!url || !token" @click="pair">Connect</EButton>
        </EActions>
      </EStack>
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
      description="The champion plays an identical copy of itself, at short games, until the trial decides."
      confirm="Start the check"
      :busy="checkBusy"
      @confirm="checkHarness"
    >
      <p class="text">It takes the machine's cores for a while, usually under an hour. Other work waits its turn.</p>
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
.token {
  display: grid;
  justify-items: start;
  gap: var(--e-gap-snug);
}

.token :deep(.e-field) {
  inline-size: 100%;
}

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

/* The stamps share a column, as wide as the widest, so every title starts at one edge. */
.health {
  display: grid;
  grid-template-columns: max-content minmax(0, 1fr);
  gap: var(--e-gap-snug);
}

.health-item {
  display: grid;
  grid-column: 1 / -1;
  grid-template-columns: subgrid;
  align-items: baseline;
}

.harness-line {
  display: flex;
  flex-wrap: wrap;
  align-items: baseline;
  gap: var(--e-gap-snug);
  margin: 0;
}

.health-text {
  display: grid;
  gap: 2px;
  min-width: 0;
}

.health-title {
  margin: 0;
}

.health-text .sec-note {
  margin: 0;
  overflow-wrap: anywhere;
}
</style>
