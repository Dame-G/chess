<script setup lang="ts">
import { ref } from "vue";
import { EButton } from "@eos-ui/vue/components/EButton";
import { EForm } from "@eos-ui/vue/components/EForm";
import { ETextField } from "@eos-ui/vue/components/ETextField";
import { connect, connection } from "../api/runner";

const emit = defineEmits<{ connected: [] }>();

const url = ref(connection.url);
const token = ref(connection.token);

function submit() {
  connect(url.value, token.value);
  emit("connected");
}
</script>

<template>
  <EForm @submit="submit">
    <ETextField v-model="url" label="Runner URL" type="url" required />
    <ETextField
      v-model="token"
      label="Token"
      hint="Run `chess token` on the runner's machine and paste what it prints."
      autocomplete="off"
      required
    />
    <EButton type="submit">Connect</EButton>
  </EForm>
</template>
