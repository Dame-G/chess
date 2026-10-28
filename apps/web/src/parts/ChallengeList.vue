<script setup lang="ts">
import { ref } from "vue";
import { RouterLink } from "vue-router";
import { useToast } from "@eos-ui/vue";
import { EButton } from "@eos-ui/vue/components/EButton";
import { EList } from "@eos-ui/vue/components/EList";
import { EListRow } from "@eos-ui/vue/components/EListRow";
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
  <EList :model-value="null" label="Challenges">
    <EListRow
      v-for="c in challenges"
      :key="c.id"
      :value="c.id"
      :link-component="RouterLink"
      :to="`/players/lichess:${c.opponent.name.toLowerCase()}`"
    >
      <template #title>
        <span v-if="c.opponent.title" class="title">{{ c.opponent.title }}</span>
        {{ c.opponent.name }}
        <span v-if="c.opponent.rating" class="mono rating">{{ c.opponent.rating }}</span>
      </template>
      <template #subtitle>
        {{ c.direction === "incoming" ? "Incoming" : "Sent" }} · {{ describe(c) }}
        <template v-if="!c.playable"> · not playable here</template>
      </template>
      <template #actions>
        <template v-if="c.direction === 'incoming'">
          <EButton
            v-if="c.playable"
            variant="outline"
            size="xs"
            :loading="busy === c.id"
            @click="run(c.id, () => connections.accept(connection, c.id))"
          >
            Accept
          </EButton>
          <EButton v-else variant="outline" size="xs" :href="c.url" external>Play on lichess.org</EButton>
          <EButton variant="quiet" size="xs" @click="run(c.id, () => connections.decline(connection, c.id))">
            Decline
          </EButton>
        </template>
        <EButton v-else variant="outline" size="xs" @click="run(c.id, () => connections.cancel(connection, c.id))">
          Cancel
        </EButton>
      </template>
    </EListRow>
  </EList>
</template>

<style scoped>
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
