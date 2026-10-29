<script setup lang="ts">
import { computed } from "vue";
import type { Profile } from "../api/types";
import UiStat from "../ui/UiStat.vue";
import UiStats from "../ui/UiStats.vue";

/**
 * A profile's six key figures, in two rows of three. What needs a review reads as a dash, with the reason under it,
 * never as a zero.
 */
const props = defineProps<{ profile: Profile }>();

const reviewedText = computed(() =>
  props.profile.reviewed ? `from ${props.profile.reviewed.toLocaleString("en-GB")} reviewed` : "not reviewed yet",
);
</script>

<template>
  <UiStats :columns="3" class="stats">
    <UiStat
      label="Games"
      :value="profile.games"
      :sub="`${profile.reviewed.toLocaleString('en-GB')} reviewed`"
      size="lg"
    />
    <UiStat label="Score" :value="profile.results.overall.score" unit="%" :digits="1" size="lg" />
    <UiStat
      label="Accuracy"
      hint="accuracy"
      :value="profile.accuracy.overall"
      unit="%"
      :digits="1"
      :sub="reviewedText"
      size="lg"
    />
    <UiStat label="Avg. loss" hint="acpl" :value="profile.acpl" :digits="1" size="lg" />
    <UiStat
      label="Performance"
      hint="performance"
      :value="profile.opponents.performance == null ? null : Math.round(profile.opponents.performance)"
      size="lg"
    />
    <UiStat label="Win streak" :value="profile.games ? profile.longest_win_streak : null" size="lg" />
  </UiStats>
</template>

<style scoped>
.stats {
  row-gap: var(--sp-4);
}
</style>
