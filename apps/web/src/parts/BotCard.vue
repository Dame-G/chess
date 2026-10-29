<script setup lang="ts">
import { computed, ref } from "vue";
import { RouterLink } from "vue-router";
import { EButton } from "@eos-ui/vue/components/EButton";
import { ELink } from "@eos-ui/vue/components/ELink";
import { ENotice } from "@eos-ui/vue/components/ENotice";
import { ESwitch } from "@eos-ui/vue/components/ESwitch";
import type { SeekMode } from "../api/types";
import { speedName, timeControl, timeOfDay } from "../format";
import { useConnections } from "../stores/connections";
import UiBadge from "../ui/UiBadge.vue";
import UiEmpty from "../ui/UiEmpty.vue";
import UiField from "../ui/UiField.vue";
import UiKeyValue from "../ui/UiKeyValue.vue";
import UiKv from "../ui/UiKv.vue";
import UiMeter from "../ui/UiMeter.vue";
import UiSection from "../ui/UiSection.vue";
import UiSeg from "../ui/UiSeg.vue";
import UiTable from "../ui/UiTable.vue";
import { declineWords, rulesItems } from "./bot";
import BotSettingsDialog from "./BotSettingsDialog.vue";

/**
 * The bot: Dameless on Lichess as its BOT account. The online switch, whether it is looking for games, how many of the
 * day's games against bots are used, the rules it takes challenges by (with an edit dialog) and the challenges it
 * answered lately. Every control just calls its route; the person clicking is the one deciding.
 */
const connections = useConnections();

const conn = computed(() => connections.bot);
const bot = computed(() => conn.value.bot);
const online = computed(() => bot.value?.online === true);
const username = computed(() => conn.value.account?.username ?? null);
const rules = computed(() => (bot.value ? rulesItems(bot.value.settings) : []));
const editing = ref(false);
const busy = ref(false);
/** Why the last control did not work, said under the controls until the next try. */
const failure = ref<string | null>(null);
/** No version is saved: the server refuses to take the bot online until one is chosen. */
const noVersion = computed(() => !bot.value?.settings.version);

async function run(action: () => Promise<unknown>) {
  busy.value = true;
  failure.value = null;
  try {
    await action();
  } catch (e) {
    failure.value = e instanceof Error ? e.message : String(e);
  } finally {
    busy.value = false;
  }
}

const seekMode = computed({
  get: () => (bot.value?.seek.mode ?? "off") as SeekMode,
  set: (v: SeekMode) => void run(() => connections.setBotSeek(v)),
});
const seekOptions = computed(() =>
  [
    { value: "off", label: "Off" },
    { value: "once", label: "Find one game" },
    { value: "always", label: "Always" },
  ].map((o) => ({ ...o, value: o.value as SeekMode, disabled: !online.value || busy.value })),
);

const state = computed(() => {
  const c = conn.value;
  if (!c.configured) return "No token yet";
  if (noVersion.value && !online.value) return "Choose the version it plays first";
  if (c.state === "error") return c.error ?? "Cannot reach Lichess";
  if (!online.value) return "Offline";
  if (c.state === "connecting") return "Connecting…";
  return c.playing.length
    ? `Playing ${c.playing.length === 1 ? "a game" : `${c.playing.length} games`}`
    : "Online, waiting for a game";
});

const seekLine = computed(() => {
  const s = bot.value?.seek;
  if (!s || s.mode === "off") return null;
  if (s.pending) {
    const p = s.pending;
    return `Waiting for ${p.opponent.name} (${p.opponent.rating}) to answer a ${p.rated ? "rated" : "casual"} ${timeControl({ type: "clock", initial_ms: p.clock.initial * 1000, increment_ms: p.clock.increment * 1000 })} challenge.`;
  }
  return s.note;
});

const limit = computed(() => {
  const b = bot.value;
  if (!b) return null;
  const used = b.bot_games_today;
  return {
    used,
    reached: !!b.bot_limit_until,
    text: `${used} of 100`,
    note: b.bot_limit_until
      ? `It played 100 games against bots. It looks again at ${timeOfDay(b.bot_limit_until)}.`
      : undefined,
  };
});

interface Entry {
  id: string;
  verdict: string;
  tone: "win" | "neutral" | "info";
  who: string;
  what: string;
  why: string;
}
const entries = computed<Entry[]>(() => {
  const who = (o: { name: string; rating?: number | null }) => `${o.name}${o.rating ? ` (${o.rating})` : ""}`;
  const waiting: Entry[] = conn.value.challenges.map((c) => ({
    id: c.id,
    verdict: "Waiting",
    tone: "info",
    who: who(c.opponent),
    what: `${c.rated ? "Rated" : "Casual"} ${speedName[c.speed].toLowerCase()}${c.time ? ` ${timeControl(c.time)}` : ""}`,
    why: "Waiting for an answer",
  }));
  const answered: Entry[] = connections.recent
    .filter((e) => e.challenge.connection === "bot")
    .slice(0, 5)
    .map((e) => {
      const c = e.challenge;
      const accepted = e.event === "accepted";
      return {
        id: `${c.id}-${e.event}`,
        verdict: accepted
          ? "Accepted"
          : e.event === "canceled"
            ? "Withdrawn"
            : c.direction === "outgoing"
              ? "Declined by"
              : "Declined",
        tone: accepted ? "win" : "neutral",
        who: who(c.opponent),
        what: `${c.rated ? "Rated" : "Casual"} ${speedName[c.speed].toLowerCase()}${c.time ? ` ${timeControl(c.time)}` : ""}`,
        why: accepted ? "" : declineWords(e.reason),
      };
    });
  return [...waiting, ...answered];
});
const columns = [
  { key: "verdict", label: "Answer" },
  { key: "who", label: "Challenger" },
  { key: "what", label: "Game", hide: "sm" as const },
  { key: "why", label: "Why", hide: "sm" as const },
];
</script>

<template>
  <div id="d-bot" class="bot">
    <UiEmpty v-if="connections.loaded && !conn.configured" dense title="Dameless has no BOT account token yet.">
      <template #actions>
        <ELink :link-component="RouterLink" to="/settings?tab=accounts">Add one in Settings</ELink>
      </template>
    </UiEmpty>
    <template v-else-if="bot">
      <UiSection title="On Lichess">
        <template #aside>
          <span v-if="username" class="who">
            <ELink :href="`https://lichess.org/@/${username}`" external>{{ username }}</ELink>
            <span v-if="connections.botRating" class="rating"> · {{ connections.botRating }} blitz</span>
          </span>
        </template>
        <div class="controls">
          <ESwitch
            :model-value="online"
            label="Play on Lichess"
            :description="state"
            :disabled="busy || (noVersion && !online)"
            @update:model-value="run(() => connections.setBotOnline($event))"
          />

          <ENotice v-if="failure" tone="warning" title="That did not work" role="alert">{{ failure }}</ENotice>

          <UiField group label="Looking for games">
            <UiSeg v-model="seekMode" label="Looking for games" :options="seekOptions" />
          </UiField>
          <p v-if="seekLine" class="line" role="status">{{ seekLine }}</p>

          <UiMeter
            v-if="limit"
            label="Games against bots in the last day"
            :value="limit.used"
            :max="100"
            :text="limit.text"
            :tone="limit.reached ? 'warn' : undefined"
          />
          <p v-if="limit?.note" class="line">{{ limit.note }}</p>
        </div>
      </UiSection>

      <UiSection title="Rules">
        <template #aside>
          <EButton :variant="noVersion ? 'primary' : 'outline'" size="sm" @click="editing = true">
            <template v-if="noVersion">Choose a version</template>
            <template v-else>Edit rules</template>
          </EButton>
        </template>
        <p v-if="noVersion" class="line" role="status">
          No version chosen: the bot won't play. Choose the version that plays in its rules.
        </p>
        <UiKeyValue dense>
          <UiKv v-for="r in rules" :key="r.label" :label="r.label" :tone="r.tone">
            {{ r.value }}
          </UiKv>
        </UiKeyValue>
      </UiSection>

      <UiSection title="Recent challenges">
        <UiEmpty v-if="!entries.length" dense title="Challenges it answers show here, with why it said no." />
        <UiTable v-else label="Recent challenges" :columns="columns" :rows="entries" row-key="id">
          <template #cell-verdict="{ row }">
            <UiBadge :tone="row.tone">{{ row.verdict }}</UiBadge>
          </template>
          <template #cell-why="{ row }">{{ row.why || "–" }}</template>
        </UiTable>
      </UiSection>

      <BotSettingsDialog v-if="bot" v-model:open="editing" :settings="bot.settings" />
    </template>
  </div>
</template>

<style scoped>
.bot {
  min-width: 0;
}

.controls {
  display: grid;
  gap: var(--sp-3);
}

.who {
  font: var(--t-meta);
}

.rating {
  color: var(--c-ink-2);
}

.line {
  margin: 0;
  color: var(--c-ink-2);
  font: var(--t-meta);
}
</style>
