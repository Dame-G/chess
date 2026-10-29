<script setup lang="ts">
import { ref } from "vue";
import { useToast } from "@eos-ui/vue";
import { EButton } from "@eos-ui/vue/components/EButton";
import type { Challenge, ConnectionKey } from "../api/types";
import { speedName, timeControl } from "../format";
import { useConnections } from "../stores/connections";
import type { PlayerRef } from "../ui/player";
import UiPlayer from "../ui/UiPlayer.vue";

/** Challenges waiting on an account: incoming ones to accept or decline, outgoing ones to cancel. */
defineProps<{ challenges: Challenge[]; connection: ConnectionKey }>();

const connections = useConnections();
const toast = useToast();
const busy = ref<string | null>(null);

async function run(id: string, action: () => Promise<unknown>) {
  busy.value = id;
  try {
    await action();
  } catch (e) {
    toast.show({ title: "That did not work", message: e instanceof Error ? e.message : String(e), tone: "warning" });
  } finally {
    busy.value = null;
  }
}

const who = (c: Challenge): PlayerRef => ({
  name: c.opponent.name,
  key: `lichess:${c.opponent.name.toLowerCase()}`,
  title: c.opponent.title ?? null,
  rating: c.opponent.rating ?? null,
});

const describe = (c: Challenge) =>
  [
    c.rated ? "Rated" : "Casual",
    speedName[c.speed],
    c.time ? timeControl(c.time) : "",
    c.variant !== "standard" ? c.variant : "",
  ]
    .filter(Boolean)
    .join(" · ");
</script>

<template>
  <ul class="list" aria-label="Challenges">
    <li v-for="c in challenges" :key="c.id" class="row">
      <div class="text">
        <UiPlayer :seat="who(c)" link />
        <p class="meta">
          {{ c.direction === "incoming" ? "Incoming" : "Sent" }} · {{ describe(c) }}
          <template v-if="!c.playable"> · not playable here</template>
        </p>
      </div>
      <div class="acts">
        <template v-if="c.direction === 'incoming'">
          <EButton
            v-if="c.playable"
            variant="outline"
            size="sm"
            :loading="busy === c.id"
            @click="run(c.id, () => connections.accept(connection, c.id))"
          >
            Accept
          </EButton>
          <EButton v-else variant="outline" size="sm" :href="c.url" external>Play on lichess.org</EButton>
          <EButton variant="quiet" size="sm" @click="run(c.id, () => connections.decline(connection, c.id))">
            Decline
          </EButton>
        </template>
        <EButton v-else variant="quiet" size="sm" @click="run(c.id, () => connections.cancel(connection, c.id))">
          Cancel
        </EButton>
      </div>
    </li>
  </ul>
</template>

<style scoped>
.list {
  display: grid;
  margin: 0;
  padding: 0;
  list-style: none;
}

.row {
  display: flex;
  flex-wrap: wrap;
  align-items: center;
  justify-content: space-between;
  gap: var(--sp-2) var(--sp-3);
  min-height: var(--h-row-2);
  padding-block: var(--sp-2);
}

.row + .row {
  border-block-start: var(--e-stroke-hair) solid var(--c-line);
}

.text {
  display: grid;
  flex: 1 1 14rem;
  gap: var(--sp-1);
  min-width: 0;
}

.meta {
  margin: 0;
  color: var(--c-ink-2);
  font: var(--t-meta);
}

.acts {
  display: flex;
  flex: none;
  gap: var(--sp-1);
}
</style>
