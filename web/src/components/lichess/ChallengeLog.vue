<script setup lang="ts">
import { EStamp } from "@eos-ui/vue/components/EStamp";
import OpponentName from "./OpponentName.vue";
import { challengeWho, challenges, clockLabel, declineText, speedName, type ChallengeEvent } from "../../lichess";

const meta = (c: ChallengeEvent) => {
  const parts = [
    c.clock ? `${speedName(c.speed)} ${clockLabel(c.clock.initial, c.clock.increment)}` : speedName(c.speed),
    c.rated ? "rated" : "casual",
    c.direction === "incoming" ? "challenged you" : "you challenged them",
  ];
  if (c.decision === "declined" && c.reason) parts.push(declineText(c.reason, c.direction === "incoming"));
  return parts.join(" · ");
};
const stamp = {
  accepted: { label: "Accepted", tone: "success" },
  declined: { label: "Declined", tone: "warning" },
  canceled: { label: "Withdrawn", tone: "neutral" },
} as const;
</script>

<template>
  <ul v-if="challenges.length" class="log" aria-label="Recent challenges">
    <li v-for="c in challenges" :key="c.id" class="item">
      <span class="what">
        <OpponentName
          :name="challengeWho(c)?.name ?? 'Someone'"
          :title="challengeWho(c)?.title"
          :rating="challengeWho(c)?.rating"
        />
        <span class="meta num">{{ meta(c) }}</span>
      </span>
      <EStamp size="sm" :tone="stamp[c.decision].tone" :label="stamp[c.decision].label" />
    </li>
  </ul>
  <p v-else class="empty">Challenges the bot gets, and ones you send, are listed here as they are answered.</p>
</template>

<style scoped>
.log {
  display: grid;
  margin: 0;
  padding: 0;
  list-style: none;
}

.item {
  display: flex;
  align-items: center;
  justify-content: space-between;
  gap: var(--e-gap-base);
  padding-block: var(--e-gap-snug);
  border-block-end: var(--e-stroke-hair) solid color-mix(in oklab, var(--e-border) 40%, transparent);
}

.what {
  display: grid;
  min-width: 0;
}

.meta {
  color: var(--e-muted);
  font: var(--e-type-small);
}

.empty {
  margin: 0;
  color: var(--e-muted);
  font: var(--e-type-small);
}
</style>
