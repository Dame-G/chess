<script setup lang="ts">
import { computed, onBeforeUnmount, ref, watch } from "vue";
import { useRoute, useRouter } from "vue-router";
import { PhArrowUpRight } from "@phosphor-icons/vue";
import { EButton } from "@eos-ui/vue/components/EButton";
import { ApiError } from "../api/client";
import type { ProfileFilter } from "../api/types";
import ProfileFilters from "../parts/ProfileFilters.vue";
import ProfileHead from "../parts/ProfileHead.vue";
import ProfileOpenings from "../parts/ProfileOpenings.vue";
import ProfileStats from "../parts/ProfileStats.vue";
import ProfileView from "../parts/ProfileView.vue";
import ReviewBar from "../parts/ReviewBar.vue";
import { ALL_GAMES, usePlayers } from "../stores/players";
import UiEmpty from "../ui/UiEmpty.vue";
import UiSheetPage from "../ui/UiSheetPage.vue";
import { isPlayerKey, nameOfKey } from "./compare";

/**
 * A person on Lichess: their games added up with the same building blocks as You, on one page. When none of their games
 * are here the page is the way to bring them in (the last fifty); then comes the review; then Compare.
 */
const route = useRoute();
const router = useRouter();
const players = usePlayers();

const key = computed(() => String(route.params["key"] ?? "").toLowerCase());
const filter = ref<ProfileFilter>({ ...ALL_GAMES });
const missing = ref(false);
const failed = ref<string | null>(null);
const profile = computed(() => players.profileOf(key.value, filter.value));
const name = computed(() => profile.value?.player.name ?? nameOfKey(key.value));
/** None of their games are here, with nothing filtered out: the import is the whole page. */
const notImported = computed(
  () => profile.value?.games === 0 && !filter.value.speeds?.length && filter.value.rated == null && !filter.value.since,
);
const loading = computed(() => !profile.value && !failed.value && !missing.value);
const subtitle = computed(() => [profile.value?.player.title, "Lichess"].filter(Boolean).join(" · "));
const lichessUrl = computed(() => `https://lichess.org/@/${nameOfKey(key.value)}`);

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
  <UiSheetPage :title="name" :subtitle="subtitle" :loading="loading" :sync="false">
    <template v-if="profile && !missing && !failed" #actions>
      <EButton v-if="!notImported" variant="outline" size="sm" @click="compare">Compare with you</EButton>
      <EButton variant="quiet" size="sm" :href="lichessUrl" external aria-label="On Lichess" title="On Lichess">
        <PhArrowUpRight aria-hidden="true" />
      </EButton>
    </template>
    <template v-if="profile && !missing && !failed" #lead>
      <ProfileHead :player="profile.player" />
    </template>

    <UiEmpty
      v-if="missing"
      title="No such player"
      :body="`Lichess has no player called ${nameOfKey(key)}. Check the spelling, or paste a link to their profile in Compare.`"
    >
      <template #actions>
        <EButton variant="outline" @click="router.push('/compare')">Open Compare</EButton>
      </template>
    </UiEmpty>
    <UiEmpty v-else-if="failed" tone="error" title="This profile didn't load" :body="failed">
      <template #actions>
        <EButton variant="outline" @click="load">Try again</EButton>
      </template>
    </UiEmpty>
    <template v-else-if="profile">
      <UiEmpty
        v-if="notImported"
        :title="`No games from ${profile.player.name} yet`"
        body="Import their recent Lichess games to see how they play."
      >
        <template #actions>
          <ReviewBar
            :player-key="key"
            :name="profile.player.name"
            :games="profile.games"
            :pending="profile.pending_review"
            :total="profile.unreviewed_total"
            bare
          />
        </template>
      </UiEmpty>
      <template v-else>
        <ProfileFilters v-model="filter" :games="profile.games" />
        <UiEmpty v-if="profile.games === 0" dense title="No games match these filters." />
        <template v-else>
          <ReviewBar
            :player-key="key"
            :name="profile.player.name"
            :games="profile.games"
            :pending="profile.pending_review"
            :total="profile.unreviewed_total"
            :lead="profile.reviewed === 0"
            importable
            import-label="Import more"
          />
          <ProfileStats :profile="profile" />
          <ProfileView :profile="profile" />
          <ProfileOpenings :profile="profile" />
        </template>
      </template>
    </template>
  </UiSheetPage>
</template>
