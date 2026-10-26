<script setup lang="ts">
import { computed } from "vue";
import { useRoute, useRouter } from "vue-router";
import { ESegmented } from "@eos-ui/vue/components/ESegmented";

/** The head of the Lichess places: You, Your bot, and the comparison. The choice is the address. */
const route = useRoute();
const router = useRouter();

type Place = "me" | "bot" | "compare";
const options = [
  { value: "me", label: "You" },
  { value: "bot", label: "Your bot" },
  { value: "compare", label: "Compare" },
];
const place = computed<Place>({
  get: () => (route.name === "compare" ? "compare" : route.query.as === "bot" ? "bot" : "me"),
  set: (v) => {
    if (v === "compare") void router.push("/lichess/compare");
    else if (route.name === "compare") void router.push(v === "bot" ? "/lichess?as=bot" : "/lichess");
    else void router.replace({ query: { ...route.query, as: v === "bot" ? "bot" : undefined } });
  },
});
</script>

<template>
  <ESegmented v-model="place" label="Account" hide-label size="sm" :options="options" />
</template>
