<script setup lang="ts">
import { computed, ref } from "vue";
import { useRouter } from "vue-router";
import { useQueryCache } from "@pinia/colada";
import { EButton } from "@eos-ui/vue/components/EButton";
import { EEmptyState } from "@eos-ui/vue/components/EEmptyState";
import { ELink } from "@eos-ui/vue/components/ELink";
import { ENotice } from "@eos-ui/vue/components/ENotice";
import { ESkeleton } from "@eos-ui/vue/components/ESkeleton";
import { ESwitch } from "@eos-ui/vue/components/ESwitch";
import Workspace from "../components/Workspace.vue";
import BotSettings from "../components/lichess/BotSettings.vue";
import ChallengeForm from "../components/lichess/ChallengeForm.vue";
import ChallengeLog from "../components/lichess/ChallengeLog.vue";
import GameRows from "../components/lichess/GameRows.vue";
import SetupPanel from "../components/lichess/SetupPanel.vue";
import { api } from "../api/runner";
import { unwrap, useLichess, useLichessGames } from "../api/queries";
import type { LichessStatus } from "../lichess";

const router = useRouter();
const cache = useQueryCache();
const { state: lichess } = useLichess();
const { state: games } = useLichessGames();

const status = computed(() => lichess.value.data);
/** A bot account is connected: the page shows the bot, otherwise the steps to get one. */
const bot = computed(() => (status.value?.configured && status.value.account?.bot ? status.value : undefined));
const account = computed(() => bot.value?.account);

// ---- The switch: one control for online and offline, answered by the runner's state.
const switching = ref(false);
const problem = ref("");
async function setOnline(on: boolean) {
  switching.value = true;
  problem.value = "";
  try {
    const next = await unwrap<LichessStatus>(api.POST(on ? "/api/lichess/start" : "/api/lichess/stop"));
    cache.setQueryData<LichessStatus>(["lichess"], next);
  } catch (e) {
    problem.value = (e as Error).message;
  } finally {
    switching.value = false;
  }
}
const online = computed({
  get: () => bot.value?.online ?? false,
  set: (on: boolean) => void setOnline(on),
});
const sentence = computed(() => {
  const b = bot.value;
  if (!b?.online) return "Not accepting challenges";
  if (b.state === "connecting") return "Connecting to Lichess…";
  if (b.state === "error") return "Cannot reach Lichess, trying again";
  const n = b.playing.length;
  return n ? `Playing ${n === 1 ? "a game" : `${n} games`}` : "Accepting challenges";
});

async function disconnect() {
  problem.value = "";
  try {
    cache.setQueryData<LichessStatus>(["lichess"], await unwrap(api.DELETE("/api/lichess/token")));
  } catch (e) {
    problem.value = (e as Error).message;
  }
}

const gameList = computed(() => games.value.data ?? []);
const open = (id: number) => void router.push(`/matches/${id}`);
</script>

<template>
  <Workspace library-label="Lichess games" inspector-label="Challenges">
    <template #library-head>
      <strong class="head-title">Games</strong>
      <span v-if="gameList.length" class="count num">{{ gameList.length }}</span>
    </template>
    <template #library>
      <ESkeleton v-if="games.status === 'pending'" :lines="5" label="Loading games" class="pad" />
      <EEmptyState v-else-if="!gameList.length" title="No games yet" icon="info" size="sm" bare class="pad">
        Games the bot plays on Lichess are kept here to replay.
      </EEmptyState>
      <GameRows v-else :games="gameList" @open="open" />
    </template>

    <template v-if="bot && account" #stage-head>
      <span class="identity">
        <ELink :href="`https://lichess.org/@/${account.username}`" external>{{ account.username }}</ELink>
      </span>
      <ESwitch v-model="online" label="Online" :description="sentence" :disabled="switching" class="power" />
    </template>

    <div class="stage">
      <ESkeleton v-if="lichess.status === 'pending' && !status" :lines="6" label="Loading" class="pad" />
      <ENotice
        v-else-if="lichess.status === 'error'"
        tone="warning"
        title="The runner does not know Lichess yet"
        :role="false"
      >
        {{ lichess.error?.message }}
      </ENotice>
      <SetupPanel v-else-if="status && !bot" :status="status" />
      <template v-else-if="bot">
        <ENotice
          v-if="bot.state === 'error' && bot.error"
          tone="danger"
          title="Lichess refused the connection"
          :role="false"
        >
          {{ bot.error }}
        </ENotice>
        <ENotice v-if="problem" tone="danger" title="That did not work" :role="false">{{ problem }}</ENotice>
        <BotSettings :settings="bot.settings" />
      </template>
    </div>

    <template v-if="status?.configured" #stage-foot>
      <EButton variant="quiet" size="sm" @click="disconnect">Disconnect from Lichess</EButton>
    </template>

    <template v-if="bot" #inspector-head>
      <strong class="head-title">Challenge a bot</strong>
    </template>
    <template v-if="bot" #inspector>
      <div class="side">
        <ChallengeForm :online="bot.online" />
        <section class="log" aria-labelledby="lv-log">
          <h3 id="lv-log" class="part-title">Recent challenges</h3>
          <ChallengeLog />
        </section>
      </div>
    </template>
  </Workspace>
</template>

<style scoped>
.head-title {
  font: var(--e-type-h6);
}

.count {
  margin-inline-start: auto;
  color: var(--e-muted);
  font: var(--e-type-small);
}

.pad {
  padding: var(--e-gap-base) var(--ws-pad);
}

.identity {
  font: var(--e-type-h6);
}

.power {
  margin-inline-start: auto;
}

.stage {
  display: grid;
  align-content: start;
  gap: var(--e-gap-loose);
  padding: var(--e-gap-loose) var(--ws-pad);
}

.stage > :deep(*) {
  max-inline-size: 36rem;
}

.side {
  display: grid;
  align-content: start;
  gap: var(--e-gap-section);
  padding: var(--e-gap-loose) var(--ws-pad);
}

.log {
  display: grid;
  gap: var(--e-gap-base);
}

.part-title {
  margin: 0;
  color: var(--e-muted);
  font: var(--e-type-small);
  font-weight: 600;
  letter-spacing: 0.04em;
  text-transform: uppercase;
}
</style>
