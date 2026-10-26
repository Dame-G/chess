<script setup lang="ts">
import { ECheckboxGroup } from "@eos-ui/vue/components/ECheckboxGroup";
import { ESegmented } from "@eos-ui/vue/components/ESegmented";
import { ESwitch } from "@eos-ui/vue/components/ESwitch";
import { speedName } from "../../lichess";
import { LIMITS, SPEEDS } from "./compare";

/** Which games count: Lichess's speeds (none ticked is all), rated only, the newest N of each player. */
const speeds = defineModel<string[]>("speeds", { required: true });
const rated = defineModel<boolean>("rated", { required: true });
const limit = defineModel<string>("limit", { required: true });

const speedOptions = SPEEDS.map((value) => ({ value, label: speedName(value) }));
const limitOptions = LIMITS.map((n) => ({ value: String(n), label: String(n) }));
</script>

<template>
  <div class="filters">
    <ECheckboxGroup v-model="speeds" label="Speeds" orientation="horizontal" :options="speedOptions" />
    <ESegmented v-model="limit" label="Newest games of each player" size="sm" :options="limitOptions" />
    <ESwitch v-model="rated" label="Rated games only" />
  </div>
</template>

<style scoped>
.filters {
  display: grid;
  gap: var(--e-gap-base);
}
</style>
