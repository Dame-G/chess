<script setup lang="ts">
import { computed, ref } from "vue";
import { RouterLink } from "vue-router";
import { useToast } from "@eos-ui/vue";
import { EButton } from "@eos-ui/vue/components/EButton";
import { ELink } from "@eos-ui/vue/components/ELink";
import { ESegmented } from "@eos-ui/vue/components/ESegmented";
import { ESwitch } from "@eos-ui/vue/components/ESwitch";
import type { SeekMode } from "../api/types";
import { speedName, timeControl } from "../format";
import { useConnections } from "../stores/connections";
import { declineWords, rulesSummary } from "./bot";
import BotSettingsDialog from "./BotSettingsDialog.vue";

/**
 * Out in the world: Dameless on Lichess as its BOT account. The online switch, its rating, whether it is looking for
 * games, the rules it takes challenges by (with an edit dialog) and the challenges it answered lately. Every control
 * just calls its route; the person clicking is the one deciding.
 */
const connections = useConnections();
const toast = useToast();

const conn = computed(() => connections.bot);
const bot = computed(() => conn.value.bot);
const online = computed(() => bot.value?.online === true);
const editing = ref(false);
const busy = ref(false);

async function run(action: () => Promise<unknown>) {
  busy.value = true;
  try {
    await action();
  } catch (e) {
    toast.show({ title: "That did not work", message: e instanceof Error ? e.message : String(e), tone: "warning" });
  } finally {
    busy.value = false;
  }
}

const seekMode = computed({
  get: () => bot.value?.seek.mode ?? "off",
  set: (v: string | undefined) => {
    if (v) void run(() => connections.setBotSeek(v as SeekMode));
  },
});

const state = computed(() => {
  const c = conn.value;
  if (!c.configured) return "No token yet";
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
  if (b.bot_limit_until)
    return `Played 100 games against bots today. It looks again at ${new Date(b.bot_limit_until).toLocaleTimeString("en-GB", { hour: "2-digit", minute: "2-digit" })}.`;
  return `${b.bot_games_today} of 100 games against bots in the last day.`;
});

const recent = computed(() => connections.recent.filter((e) => e.challenge.connection === "bot").slice(0, 5));
const answered = (e: (typeof recent.value)[number]) => {
  const c = e.challenge;
  const who = `${c.opponent.name}${c.opponent.rating ? ` (${c.opponent.rating})` : ""}`;
  const what = `${c.rated ? "rated" : "casual"} ${speedName[c.speed].toLowerCase()}${c.time ? ` ${timeControl(c.time)}` : ""}`;
  const rest = `${who}, ${what}`;
  if (e.event === "accepted") return { verdict: "Accepted", good: true, rest };
  const why = declineWords(e.reason);
  const verdict = e.event === "canceled" ? "Withdrawn" : c.direction === "outgoing" ? "Declined by" : "Declined";
  return { verdict, good: false, rest: `${rest}${why ? ` · ${why}` : ""}` };
};
</script>

<template>
  <section class="world" aria-labelledby="w-title">
    <div class="sec-head">
      <h2 id="w-title" class="part-title">Out in the world</h2>
      <p class="sec-note">
        <template v-if="conn.account">
          <ELink :href="`https://lichess.org/@/${conn.account?.username}`" external>{{ conn.account?.username }}</ELink>
          <span v-if="connections.botRating">
            · <span class="mono">{{ connections.botRating }}</span> blitz</span
          >
        </template>
        <template v-else>No Lichess account linked</template>
      </p>
    </div>

    <p v-if="!conn.configured" class="sec-note">
      Dameless has no BOT account token yet.
      <ELink :link-component="RouterLink" to="/settings">Add one in Settings</ELink>.
    </p>
    <template v-else-if="bot">
      <div class="line">
        <ESwitch
          :model-value="online"
          label="Play on Lichess"
          :description="state"
          :disabled="busy"
          @update:model-value="run(() => connections.setBotOnline($event))"
        />
      </div>

      <div class="line">
        <ESegmented
          v-model="seekMode"
          label="Looking for games"
          size="sm"
          :disabled="!online || busy"
          :options="[
            { value: 'off', label: 'Off' },
            { value: 'once', label: 'Find one game' },
            { value: 'always', label: 'Keep playing' },
          ]"
        />
        <p v-if="seekLine" class="sec-note" role="status">{{ seekLine }}</p>
        <p v-if="limit && seekMode !== 'off'" class="sec-note">{{ limit }}</p>
      </div>

      <div class="rules">
        <p class="rules-text">{{ rulesSummary(bot.settings) }}</p>
        <EButton variant="outline" size="sm" @click="editing = true">Edit rules</EButton>
      </div>

      <div v-if="conn.challenges.length || recent.length" class="recent">
        <h3 class="part-title">Recent challenges</h3>
        <ul class="plain">
          <li v-for="c in conn.challenges" :key="c.id" class="sec-note">
            {{ c.opponent.name }}{{ c.opponent.rating ? ` (${c.opponent.rating})` : "" }} is waiting for an answer
          </li>
          <li v-for="e in recent" :key="e.challenge.id" class="entry">
            <span :class="answered(e).good ? 'yes' : 'no'">{{ answered(e).verdict }}</span>
            {{ answered(e).rest }}
          </li>
        </ul>
      </div>
      <p v-else class="sec-note">Challenges it answers show here, with why it said no.</p>

      <BotSettingsDialog v-model:open="editing" :settings="bot.settings" />
    </template>
  </section>
</template>

<style scoped>
.world {
  display: grid;
  gap: var(--e-gap-snug);
  min-width: 0;
}

.line {
  display: grid;
  gap: var(--e-gap-tight);
}

.rules {
  display: flex;
  flex-wrap: wrap;
  align-items: center;
  justify-content: space-between;
  gap: var(--e-gap-snug);
  padding-block: var(--e-gap-snug);
  border-block: var(--line);
}

.rules-text {
  flex: 1 1 16rem;
  margin: 0;
}

.recent {
  display: grid;
  gap: var(--e-gap-tight);
}

.entry .yes,
.entry .no {
  font-weight: 600;
}

.entry .yes {
  color: var(--e-success);
}

.entry .no {
  color: var(--e-ink);
}

.entry {
  padding-block: 0.25rem;
  border-block-end: var(--line);
  color: var(--e-muted);
  font: var(--e-type-small);
}
</style>
