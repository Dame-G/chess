<script setup lang="ts">
import { ref } from "vue";
import { useQueryCache } from "@pinia/colada";
import { EButton } from "@eos-ui/vue/components/EButton";
import { EDisclosure } from "@eos-ui/vue/components/EDisclosure";
import { ELink } from "@eos-ui/vue/components/ELink";
import { ENotice } from "@eos-ui/vue/components/ENotice";
import { EPasswordField } from "@eos-ui/vue/components/EPasswordField";
import { api } from "../../api/runner";
import { unwrap } from "../../api/queries";
import { personalTokenUrl, tokenScopes, type MeStatus } from "../../lichess";

defineProps<{ problem?: string }>();

const cache = useQueryCache();
const busy = ref(false);
const error = ref("");
const token = ref("");
const saving = ref(false);

/** Lichess asks for the login in its own page, then sends the browser back here. */
async function login() {
  busy.value = true;
  error.value = "";
  try {
    const { url } = await unwrap(api.POST("/api/lichess/me/login", { body: { return_to: window.location.href } }));
    window.location.assign(url);
  } catch (e) {
    error.value = (e as Error).message;
    busy.value = false;
  }
}

async function useToken() {
  const value = token.value.trim();
  if (!value) return;
  saving.value = true;
  error.value = "";
  try {
    cache.setQueryData<MeStatus>(
      ["lichess-me"],
      await unwrap(api.PUT("/api/lichess/me/token", { body: { token: value } })),
    );
    token.value = "";
  } catch (e) {
    error.value = (e as Error).message;
  } finally {
    saving.value = false;
  }
}
</script>

<template>
  <div class="login">
    <header class="head">
      <h2 class="title">Play on Lichess as yourself</h2>
      <p class="lede">
        Log in to play rapid and classical games, challenge friends or Stockfish, and keep your games here to replay.
        The login stays on this computer.
      </p>
    </header>

    <ENotice v-if="problem" tone="danger" title="Lichess did not log you in" :role="false">{{ problem }}</ENotice>
    <ENotice v-if="error" tone="danger" title="That did not work" :role="false">{{ error }}</ENotice>

    <div class="act">
      <EButton :loading="busy" @click="login">Log in with Lichess</EButton>
    </div>

    <EDisclosure title="Use a personal token instead">
      <form class="token" @submit.prevent="useToken">
        <p class="note">
          Create a token with <span v-for="s in tokenScopes" :key="s" class="scope num">{{ s }}</span> allowed.
          <ELink :href="personalTokenUrl" external>Create it on Lichess</ELink>
        </p>
        <EPasswordField v-model="token" label="Token" mode="current" placeholder="lip_…" />
        <EButton type="submit" variant="outline" :loading="saving" :disabled="!token.trim()">Use this token</EButton>
      </form>
    </EDisclosure>
  </div>
</template>

<style scoped>
.login {
  display: grid;
  align-content: start;
  gap: var(--e-gap-loose);
}

.head {
  display: grid;
  gap: var(--e-gap-snug);
}

.title {
  margin: 0;
  font: var(--e-type-h4);
}

.lede {
  margin: 0;
  color: var(--e-muted);
}

.act {
  display: flex;
}

.token {
  display: grid;
  justify-items: start;
  gap: var(--e-gap-base);
}

.token :deep(.e-field) {
  inline-size: 100%;
}

.note {
  margin: 0;
  color: var(--e-muted);
}

.scope {
  margin-inline-end: var(--e-gap-snug);
  color: var(--e-text);
  font: var(--e-type-small);
}
</style>
