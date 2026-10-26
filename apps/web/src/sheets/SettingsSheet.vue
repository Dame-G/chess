<script setup lang="ts">
import { computed, onMounted, ref } from "vue";
import { useToast } from "@eos-ui/vue";
import { EActions } from "@eos-ui/vue/components/EActions";
import { EButton } from "@eos-ui/vue/components/EButton";
import { ECopyButton } from "@eos-ui/vue/components/ECopyButton";
import { EDescriptionList } from "@eos-ui/vue/components/EDescriptionList";
import { ESegmented } from "@eos-ui/vue/components/ESegmented";
import { EStack } from "@eos-ui/vue/components/EStack";
import { ETextField } from "@eos-ui/vue/components/ETextField";
import { connect, connection } from "../api/client";
import type { ConnectionKey, PruneResult } from "../api/types";
import { bytes, plural } from "../format";
import ConfirmDialog from "../parts/ConfirmDialog.vue";
import { choice, type Choice } from "../scheme";
import { useConnections } from "../stores/connections";
import { useDameless } from "../stores/dameless";
import { usePolicy } from "../stores/policy";
import SheetFrame from "./SheetFrame.vue";

/**
 * Settings: the two Lichess connections (your login, the bot's token), the machine (cores, the engine checkout, the
 * binaries kept and the disk they take), the theme, and the token the hosted app needs. Logging out and disconnecting
 * ask first; so does freeing disk space, after saying what it would free.
 */
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
const stateWord = (c: { configured: boolean; state: string; error: string | null }) =>
  !c.configured
    ? "Not linked"
    : c.state === "online"
      ? "Connected"
      : c.state === "connecting"
        ? "Connecting…"
        : c.state === "error"
          ? (c.error ?? "Cannot reach Lichess")
          : "Linked, offline";
const lichessItems = (c: typeof you.value) => [
  {
    label: "Account",
    text: c.account ? `${c.account.username}${c.account.title ? ` (${c.account.title})` : ""}` : "None",
  },
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
      text: m ? `${plural(m.builds.count, "binary", "binaries")}, ${bytes(m.builds.bytes)}` : "Unknown",
    },
  ];
});

const pruning = ref(false);
const plan = ref<PruneResult | null>(null);
const pruneBusy = ref(false);
async function planPrune() {
  pruneBusy.value = true;
  const r = await attempt("The check did not work", () => dameless.prune(true));
  pruneBusy.value = false;
  if (!r) return;
  if (r.builds === 0) {
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
      message: `${plural(r.builds, "old binary", "old binaries")} removed.`,
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
  <SheetFrame
    title="Settings"
    description="Lichess connections, the machine, the theme and the token for the hosted app."
  >
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
          <EButton variant="secondary" :disabled="!url || !token" @click="pair">Connect</EButton>
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
      v-model:open="pruning"
      title="Free disk space?"
      :description="
        plan ? `${plural(plan.builds, 'old binary', 'old binaries')}, about ${bytes(plan.bytes)}.` : undefined
      "
      :confirm="plan ? `Free ${bytes(plan.bytes)}` : 'Free'"
      :busy="pruneBusy"
      @confirm="prune"
    >
      <p class="text">Only binaries that no test, tune, game or the bot uses are removed. Nothing else is touched.</p>
    </ConfirmDialog>
  </SheetFrame>
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
</style>
