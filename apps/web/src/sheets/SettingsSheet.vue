<script setup lang="ts">
import { computed, ref } from "vue";
import { EActions } from "@eos-ui/vue/components/EActions";
import { EButton } from "@eos-ui/vue/components/EButton";
import { EDescriptionList } from "@eos-ui/vue/components/EDescriptionList";
import { ESegmented } from "@eos-ui/vue/components/ESegmented";
import { EStack } from "@eos-ui/vue/components/EStack";
import { ETextField } from "@eos-ui/vue/components/ETextField";
import { connect, connection } from "../api/client";
import { choice, type Choice } from "../scheme";
import { usePolicy } from "../stores/policy";
import SheetFrame from "./SheetFrame.vue";

/**
 * Settings: Lichess connections, the machine (cores, the engine checkout, builds and disk), defaults, theme, and the
 * token for the hosted app. For now: the theme, what this page is connected to, and (on the hosted site) pairing
 * with a server on this machine.
 */
const policy = usePolicy();
const themes: { value: Choice; label: string }[] = [
  { value: "system", label: "System" },
  { value: "light", label: "Light" },
  { value: "dark", label: "Dark" },
];
const facts = computed(() => [
  { label: "Server", text: connection.url || "The server this page came from" },
  { label: "Version", text: policy.status?.version ?? "Unknown" },
  { label: "Cores", text: policy.status ? String(policy.status.cores) : "Unknown" },
]);

/** Only a page from another origin (the hosted site) pairs with a server; one the server serves needs nothing. */
const remote = computed(() => connection.url !== "");
const url = ref(connection.url);
const token = ref("");
function pair() {
  connect(url.value, token.value);
  location.reload();
}
</script>

<template>
  <SheetFrame title="Settings" description="Lichess connections, the machine, defaults and the theme.">
    <EStack gap="loose">
      <ESegmented v-model="choice" label="Theme" :options="themes" />
      <EDescriptionList :items="facts" />
      <EStack v-if="remote" gap="base">
        <ETextField v-model="url" label="Server" placeholder="http://127.0.0.1:7979" autocomplete="off" />
        <ETextField
          v-model="token"
          label="Token"
          hint="Run dameless token on the machine with the server, and paste what it prints."
          autocomplete="off"
        />
        <EActions>
          <EButton variant="secondary" :disabled="!url || !token" @click="pair">Connect</EButton>
        </EActions>
      </EStack>
    </EStack>
  </SheetFrame>
</template>
