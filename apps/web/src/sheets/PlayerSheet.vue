<script setup lang="ts">
import { computed, onBeforeUnmount, ref, watch } from "vue";
import { useRoute, useRouter } from "vue-router";
import { ESheet } from "@eos-ui/vue/components/ESheet";
import { EButton } from "@eos-ui/vue/components/EButton";
import { EEmptyState } from "@eos-ui/vue/components/EEmptyState";
import { ENotice } from "@eos-ui/vue/components/ENotice";
import { ApiError } from "../api/client";
import type { ProfileFilter } from "../api/types";
import ProfileFilters from "../parts/ProfileFilters.vue";
import ProfileHead from "../parts/ProfileHead.vue";
import ProfileSkeleton from "../parts/ProfileSkeleton.vue";
import ProfileView from "../parts/ProfileView.vue";
import ReviewBar from "../parts/ReviewBar.vue";
import { ALL_GAMES, usePlayers } from "../stores/players";
import { isPlayerKey, nameOfKey } from "./compare";
import SheetBack from "./SheetBack.vue";
import { useSheet } from "./useSheet";

/**
 * A person on Lichess: their games added up with the same building blocks as You. When none of their games are here,
 * the first step is to import the last fifty; then to review them; then Compare.
 */
const sheet = useSheet();
const route = useRoute();
const router = useRouter();
const players = usePlayers();

const key = computed(() => String(route.params["key"] ?? "").toLowerCase());
const filter = ref<ProfileFilter>({ ...ALL_GAMES });
const missing = ref(false);
const failed = ref<string | null>(null);
const profile = computed(() => players.profileOf(key.value, filter.value));
const name = computed(() => profile.value?.player.name ?? nameOfKey(key.value));

async function load() {
  missing.value = false;
  failed.value = null;
  if (!isPlayerKey(key.value)) {
    missing.value = true;
    return;
  }
  try {
    await players.loadProfile(key.value, filter.value);
  } catch (e) {
    if (e instanceof ApiError && e.status === 404) missing.value = true;
    else failed.value = e instanceof Error ? e.message : String(e);
  }
}
let loaded = key.value;
watch(
  [key, filter],
  () => {
    if (loaded !== key.value) players.unwatch(loaded);
    loaded = key.value;
    void load();
  },
  { immediate: true, deep: true },
);
onBeforeUnmount(() => players.unwatch(loaded));

const compare = () => router.push({ path: "/compare", query: { players: `you,${key.value}` } });
</script>

<template>
  <ESheet v-bind="sheet.bind" :title="name" description="A player on Lichess, from the games brought in here.">
    <template #leading><SheetBack :show="sheet.canGoBack" @back="sheet.back" /></template>
    <EEmptyState v-if="missing" title="No such player">
      Lichess has no player called {{ nameOfKey(key) }}. Check the spelling, or paste a link to their profile in
      Compare.
    </EEmptyState>
    <ENotice v-else-if="failed" tone="danger" title="This profile did not load">
      {{ failed }}
      <template #actions><EButton variant="outline" size="sm" @click="load">Try again</EButton></template>
    </ENotice>
    <ProfileSkeleton v-else-if="!profile" :label="`Loading ${name}`" />
    <template v-else>
      <ProfileHead :player="profile.player" hide-name />
      <section class="sec" aria-label="Games and review">
        <ProfileFilters v-if="profile.games" v-model="filter" />
        <ReviewBar
          :player-key="key"
          :name="profile.player.name"
          :games="profile.games"
          :pending="profile.pending_review"
        />
      </section>
      <ProfileView :profile="profile" />
      <section class="sec" aria-label="Compare">
        <div class="sheet-actions">
          <EButton variant="outline" @click="compare">Compare with you</EButton>
        </div>
      </section>
    </template>
  </ESheet>
</template>
