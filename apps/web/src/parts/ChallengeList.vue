<script setup lang="ts">
import { ref } from "vue";
import { useToast } from "@eos-ui/vue";
import { EButton } from "@eos-ui/vue/components/EButton";
import { ELink } from "@eos-ui/vue/components/ELink";
import type { Challenge, ConnectionKey } from "../api/types";
import { speedName, timeControl } from "../format";
import { useConnections } from "../stores/connections";

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
  <ul class="plain list">
    <li v-for="c in challenges" :key="c.id" class="item">
      <div class="who">
        <span class="name">
          <span v-if="c.opponent.title" class="title">{{ c.opponent.title }}</span>
          {{ c.opponent.name }}
          <span v-if="c.opponent.rating" class="mono rating">{{ c.opponent.rating }}</span>
        </span>
        <span class="sec-note">
          {{ c.direction === "incoming" ? "challenges you" : "you challenged them" }} · {{ describe(c) }}
          <template v-if="!c.playable"> · <ELink :href="c.url" external>play it on lichess.org</ELink> </template>
        </span>
      </div>
      <div class="sheet-actions">
        <template v-if="c.direction === 'incoming'">
          <EButton
            variant="secondary"
            size="sm"
            :disabled="!c.playable"
            :loading="busy === c.id"
            @click="run(c.id, () => connections.accept(connection, c.id))"
          >
            Accept
          </EButton>
          <EButton variant="quiet" size="sm" @click="run(c.id, () => connections.decline(connection, c.id))">
            Decline
          </EButton>
        </template>
        <EButton v-else variant="outline" size="sm" @click="run(c.id, () => connections.cancel(connection, c.id))">
          Cancel
        </EButton>
      </div>
    </li>
  </ul>
</template>

<style scoped>
.list {
  gap: 0;
}

.item {
  display: flex;
  flex-wrap: wrap;
  align-items: center;
  justify-content: space-between;
  gap: var(--e-gap-snug);
  padding-block: var(--e-gap-snug);
  border-block-end: var(--line);
}

.who {
  display: grid;
  min-width: 0;
}

.name {
  font-weight: 600;
}

.title {
  margin-inline-end: 0.25rem;
  color: var(--e-primary);
  font: var(--e-type-small);
  font-weight: 700;
}

.rating {
  margin-inline-start: 0.25rem;
  color: var(--e-muted);
  font-weight: 400;
}
</style>
