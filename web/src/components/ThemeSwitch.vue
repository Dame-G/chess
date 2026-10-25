<script setup lang="ts">
import { computed } from "vue";
import { PhDesktop, PhMoon, PhSun } from "@phosphor-icons/vue";
import { EIconButton } from "@eos-ui/vue/components/EIconButton";
import { EMenu } from "@eos-ui/vue/components/EMenu";
import { choice, type Choice } from "../scheme";

const names: Record<Choice, string> = { system: "System", light: "Light", dark: "Dark" };
const icons = { system: PhDesktop, light: PhSun, dark: PhMoon };

/** The glyph shows the choice, so a glance says whether the page follows the system. */
const icon = computed(() => icons[choice.value]);
const options = (["system", "light", "dark"] as const).map((value) => ({ value, label: names[value] }));
const items = computed(() => [
  {
    type: "radio-group" as const,
    label: "Theme",
    value: choice.value,
    options,
    onChange: (value: string) => (choice.value = value as Choice),
  },
]);
</script>

<template>
  <EMenu label="Theme" :items="items" align="end">
    <template #trigger>
      <EIconButton :icon="icon" :label="`Theme: ${names[choice]}`" size="sm" tooltip />
    </template>
  </EMenu>
</template>
