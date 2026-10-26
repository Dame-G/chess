<script setup lang="ts">
import { computed } from "vue";
import { RouterLink } from "vue-router";
import { ELink } from "@eos-ui/vue/components/ELink";
import { ENotice } from "@eos-ui/vue/components/ENotice";
import OpponentName from "./OpponentName.vue";
import { ratingList, speedName, type MeStatus } from "../../lichess";

const props = defineProps<{ me: MeStatus; opponent?: string }>();

const account = computed(() => props.me.account);
const profile = computed(() => `https://lichess.org/@/${account.value?.username ?? ""}`);
const ratings = computed(() => ratingList(account.value?.ratings));

/** One sentence on what the account is doing, led by its state. */
const sentence = computed(() => {
  const m = props.me;
  if (m.state === "connecting") return "Connecting to Lichess…";
  if (m.state === "error") return "Cannot reach Lichess, trying again";
  if (m.playing.length) return props.opponent ? `Playing against ${props.opponent}` : "Playing a game";
  if (m.elsewhere.length) return "Playing on lichess.org";
  if (m.seek.seeking) return "Looking for an opponent";
  return "Ready to play";
});
</script>

<template>
  <section v-if="account" class="me" aria-label="Your Lichess account">
    <div class="who">
      <OpponentName :name="account.username" :title="account.title" class="name" />
      <ELink :href="profile" external>Profile</ELink>
    </div>
    <p :class="['state', `state--${me.state}`]" role="status">
      <span class="dot" aria-hidden="true"></span>
      <span>{{ sentence }}</span>
      <RouterLink v-if="me.playing[0]" :to="`/matches/${me.playing[0]}`">Open the game</RouterLink>
      <ELink v-else-if="me.elsewhere[0]" :href="me.elsewhere[0].url" external>Open on Lichess</ELink>
    </p>
    <dl v-if="ratings.length" class="ratings">
      <div v-for="r in ratings" :key="r.speed" class="rating">
        <dt>{{ speedName(r.speed) }}</dt>
        <dd class="num">{{ r.rating }}</dd>
      </div>
    </dl>
    <ENotice v-if="me.state === 'error' && me.error" tone="danger" title="Lichess refused the connection" :role="false">
      {{ me.error }}
    </ENotice>
    <ENotice v-if="me.missing_scopes.length" tone="warning" title="This login lacks permissions" :role="false">
      Lichess did not allow {{ me.missing_scopes.join(", ") }}, so some things will not work. Log out and log in again,
      allowing everything it asks for.
    </ENotice>
  </section>
</template>

<style scoped>
.me {
  display: grid;
  gap: var(--e-gap-base);
}

.who {
  display: flex;
  align-items: baseline;
  gap: var(--e-gap-base);
}

.name {
  font: var(--e-type-h4);
}

.state {
  display: flex;
  flex-wrap: wrap;
  align-items: center;
  gap: var(--e-gap-snug) var(--e-gap-base);
  margin: 0;
  color: var(--e-muted);
}

.dot {
  inline-size: 0.5rem;
  block-size: 0.5rem;
  border-radius: 50%;
  background: var(--e-muted);
}

.state--online .dot {
  background: var(--e-success);
}

.state--error .dot {
  background: var(--e-danger);
}

.ratings {
  display: flex;
  flex-wrap: wrap;
  gap: var(--e-gap-snug) var(--e-gap-loose);
  margin: 0;
}

.rating {
  display: flex;
  align-items: baseline;
  gap: var(--e-gap-snug);
}

.rating dt {
  color: var(--e-muted);
  font: var(--e-type-small);
}

.rating dd {
  margin: 0;
  font-weight: 600;
}
</style>
