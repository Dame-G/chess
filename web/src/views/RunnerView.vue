<script setup lang="ts">
import { computed } from "vue";
import { useQuery } from "@pinia/colada";
import { EDescriptionList } from "@eos-ui/vue/components/EDescriptionList";
import { EGrid } from "@eos-ui/vue/components/EGrid";
import { EMetric } from "@eos-ui/vue/components/EMetric";
import { ENotice } from "@eos-ui/vue/components/ENotice";
import { EPageHeader } from "@eos-ui/vue/components/EPageHeader";
import { EStack } from "@eos-ui/vue/components/EStack";
import { api, isRemote } from "../api/runner";
import ConnectRunner from "../components/ConnectRunner.vue";

const { state, refetch } = useQuery({
  key: ["status"],
  query: async () => {
    const { data, response } = await api.GET("/api/status");
    if (!data) throw new Error(response.status === 401 ? "token" : `status ${response.status}`);
    return data;
  },
});

const status = computed(() => state.value.data);
const gib = (bytes: number) => `${(bytes / 2 ** 30).toFixed(0)} GiB`;
</script>

<template>
  <EStack gap="section">
    <EPageHeader title="Runner" description="The machine that builds, tests and analyses engines." />

    <ENotice v-if="state.status === 'error'" tone="warning" title="No runner connected">
      <template v-if="isRemote">
        Start one with <code>chess serve</code> and connect it below. Your browser may ask to allow access to devices on
        your local network.
      </template>
      <template v-else>Start one with <code>chess serve</code>.</template>
    </ENotice>
    <ConnectRunner v-if="isRemote" @connected="refetch()" />

    <template v-if="status">
      <EGrid min="12rem" gap="base">
        <EMetric label="Threads" :value="status.cpu.threads" :help="`${status.cpu.cores} cores`" />
        <EMetric label="Memory" :value="gib(status.cpu.memory)" />
        <EMetric label="Runner" :value="status.version" />
      </EGrid>
      <EDescriptionList
        :items="[
          { label: 'CPU', text: status.cpu.name },
          {
            label: 'Reckless',
            text: status.reckless_dir,
            stamp: status.reckless_found ? 'Found' : 'Missing',
            stampTone: status.reckless_found ? 'success' : 'danger',
          },
        ]"
      />
    </template>
  </EStack>
</template>
