<script setup lang="ts">
import { computed, ref } from "vue";
import { useQueryCache } from "@pinia/colada";
import { EButton } from "@eos-ui/vue/components/EButton";
import { ELink } from "@eos-ui/vue/components/ELink";
import { ENotice } from "@eos-ui/vue/components/ENotice";
import OpponentName from "./OpponentName.vue";
import { api } from "../../api/runner";
import { unwrap } from "../../api/queries";
import { challengeLine, speedName, type MeChallenge, type MeStatus } from "../../lichess";

const props = defineProps<{ me: MeStatus }>();

const cache = useQueryCache();
const busy = ref("");
const error = ref("");

const lists = computed(() =>
  [
    { key: "incoming", title: "Challenging you", items: props.me.challenges.incoming },
    { key: "outgoing", title: "You challenged", items: props.me.challenges.outgoing },
  ].filter((l) => l.items.length),
);

async function run(id: string, call: () => Promise<MeStatus>) {
  busy.value = id;
  error.value = "";
  try {
    cache.setQueryData<MeStatus>(["lichess-me"], await call());
  } catch (e) {
    error.value = (e as Error).message;
  } finally {
    busy.value = "";
  }
}
const path = (id: string) => ({ params: { path: { id } } });
const accept = (c: MeChallenge) =>
  run(c.id, () => unwrap(api.POST("/api/lichess/me/challenges/{id}/accept", path(c.id))));
const decline = (c: MeChallenge) =>
  run(c.id, () => unwrap(api.POST("/api/lichess/me/challenges/{id}/decline", { ...path(c.id), body: {} })));
const cancel = (c: MeChallenge) => run(c.id, () => unwrap(api.DELETE("/api/lichess/me/challenges/{id}", path(c.id))));

const meta = (c: MeChallenge) =>
  `${speedName(c.speed)} ${challengeLine(c)} · you play ${c.color === "random" ? "either side" : c.color}`;
</script>

<template>
  <div class="challenges">
    <ENotice v-if="error" tone="danger" title="That did not work" :role="false">{{ error }}</ENotice>
    <p v-if="!lists.length" class="empty">No challenges waiting.</p>
    <section v-for="l in lists" :key="l.key" :aria-labelledby="`mc-${l.key}`">
      <h4 :id="`mc-${l.key}`" class="sub">{{ l.title }}</h4>
      <ul class="list">
        <li v-for="c in l.items" :key="c.id" class="item">
          <span class="what">
            <OpponentName v-bind="c.opponent" />
            <span class="meta num">{{ meta(c) }}</span>
          </span>
          <span class="acts">
            <template v-if="l.key === 'incoming'">
              <ELink v-if="!c.playable" :href="c.url" external>Accept on Lichess</ELink>
              <EButton v-else size="sm" :loading="busy === c.id" @click="accept(c)">Accept</EButton>
              <EButton size="sm" variant="quiet" :disabled="busy === c.id" @click="decline(c)">Decline</EButton>
            </template>
            <EButton v-else size="sm" variant="quiet" :loading="busy === c.id" @click="cancel(c)">Cancel</EButton>
          </span>
        </li>
      </ul>
    </section>
  </div>
</template>

<style scoped>
.challenges {
  display: grid;
  gap: var(--e-gap-base);
}

.sub {
  margin: 0;
  color: var(--e-muted);
  font: var(--e-type-small);
}

.list {
  display: grid;
  margin: 0;
  padding: 0;
  list-style: none;
}

.item {
  display: flex;
  flex-wrap: wrap;
  align-items: center;
  justify-content: space-between;
  gap: var(--e-gap-snug) var(--e-gap-base);
  padding-block: var(--e-gap-snug);
  border-block-end: var(--e-stroke-hair) solid color-mix(in oklab, var(--e-border) 40%, transparent);
}

.what {
  display: grid;
  min-inline-size: 0;
}

.meta,
.empty {
  color: var(--e-muted);
  font: var(--e-type-small);
}

.empty {
  margin: 0;
}

.acts {
  display: flex;
  align-items: center;
  gap: var(--e-gap-snug);
}
</style>
