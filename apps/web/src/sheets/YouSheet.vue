<script setup lang="ts">
import { computed } from "vue";
import { RouterLink } from "vue-router";
import { EDescriptionList } from "@eos-ui/vue/components/EDescriptionList";
import { ELink } from "@eos-ui/vue/components/ELink";
import { yourResult } from "../format";
import { useConnections } from "../stores/connections";
import { useGames } from "../stores/games";
import SheetFrame from "./SheetFrame.vue";

/**
 * You: your rating and accuracy trend, phases, openings, time, best moments and worst blunders, weak spots to play
 * out, Compare with…, and your Lichess connection. For now: the connection and your record here; the profile
 * (`GET /api/players/you/profile`) comes next.
 */
const connections = useConnections();
const games = useGames();

const mine = computed(() => games.list.filter((g) => g.you && g.status === "finished"));
const items = computed(() => {
  const you = connections.you;
  const record = { won: 0, drew: 0, lost: 0 };
  for (const g of mine.value) {
    const r = yourResult(g);
    if (r) record[r]++;
  }
  const rows = [
    {
      label: "Lichess",
      text: you.account ? `${you.account.username} · ${you.state}` : "Not linked. Log in from Settings.",
    },
  ];
  for (const speed of ["bullet", "blitz", "rapid", "classical"] as const) {
    const r = you.account?.ratings[speed];
    if (r)
      rows.push({
        label: speed[0]!.toUpperCase() + speed.slice(1),
        text: `${r.rating}${r.provisional ? "?" : ""} · ${r.games} games`,
      });
  }
  rows.push({
    label: "Games here",
    text: `${mine.value.length}: ${record.won} won, ${record.drew} drawn, ${record.lost} lost`,
  });
  return rows;
});
</script>

<template>
  <SheetFrame
    title="You"
    description="Your games added up: trend, phases, openings, time, and the positions to work on."
  >
    <EDescriptionList :items="items" />
    <p class="next">
      The trend, phases, openings, moments and weak spots arrive with your profile.
      <ELink :link-component="RouterLink" to="/compare?players=you,dameless">Compare with Dameless</ELink>
    </p>
  </SheetFrame>
</template>

<style scoped>
.next {
  color: var(--e-muted);
  font: var(--e-type-small);
}
</style>
