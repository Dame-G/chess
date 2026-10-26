<script setup lang="ts">
import { computed, ref } from "vue";
import { useQueryCache } from "@pinia/colada";
import { EButton } from "@eos-ui/vue/components/EButton";
import { EDialog } from "@eos-ui/vue/components/EDialog";
import { ELink } from "@eos-ui/vue/components/ELink";
import { ENotice } from "@eos-ui/vue/components/ENotice";
import { EPasswordField } from "@eos-ui/vue/components/EPasswordField";
import { api } from "../../api/runner";
import { unwrap } from "../../api/queries";
import type { LichessStatus } from "../../lichess";

const props = defineProps<{ status: LichessStatus }>();

const cache = useQueryCache();
const token = ref("");
const busy = ref(false);
const error = ref("");
const upgrading = ref(false);

/** A token is stored but its account is an ordinary one: the next step is making it a bot. */
const account = computed(() => (props.status.configured ? props.status.account : undefined));

const tokenUrl = "https://lichess.org/account/oauth/token/create?scopes[]=bot:play&description=Dameless";

function take(next: LichessStatus) {
  cache.setQueryData<LichessStatus>(["lichess"], next);
}

async function run(call: () => Promise<LichessStatus>, after?: () => void) {
  busy.value = true;
  error.value = "";
  try {
    take(await call());
    after?.();
  } catch (e) {
    error.value = (e as Error).message;
  } finally {
    busy.value = false;
  }
}

const connect = () =>
  token.value.trim() &&
  run(
    () => unwrap(api.PUT("/api/lichess/token", { body: { token: token.value.trim() } })),
    () => (token.value = ""),
  );
const forget = () => run(() => unwrap(api.DELETE("/api/lichess/token")));
const upgrade = () =>
  run(
    () => unwrap(api.POST("/api/lichess/upgrade", { body: { confirm: true } })),
    () => (upgrading.value = false),
  );
</script>

<template>
  <div class="setup">
    <template v-if="account">
      <header class="head">
        <h2 class="title">{{ account.username }} is not a bot account yet</h2>
        <p class="lede">
          Lichess only lets bots play through the Bot API. Making this account a bot is
          <strong>permanent</strong>: it can never play as a person again, and Lichess only allows it for an account
          that has never played a game.
        </p>
      </header>
      <div class="row">
        <EButton variant="danger" :loading="busy" @click="upgrading = true">Make it a bot account</EButton>
        <EButton variant="quiet" :disabled="busy" @click="forget">Use another token</EButton>
      </div>
    </template>

    <template v-else>
      <header class="head">
        <h2 class="title">Play on Lichess</h2>
        <p class="lede">One of your engines plays as a bot on Lichess, against people and other bots.</p>
      </header>
      <ol class="steps">
        <li>
          <strong>Make a new Lichess account</strong> that has never played a game.
          <ELink href="https://lichess.org/signup" external>Sign up</ELink>
        </li>
        <li>
          <strong>Create a token</strong> for it that allows playing as a bot.
          <ELink :href="tokenUrl" external>Create the token</ELink>
        </li>
        <li><strong>Paste it here.</strong> It stays on this computer.</li>
      </ol>
      <form class="form" @submit.prevent="connect">
        <EPasswordField v-model="token" label="Token" mode="current" placeholder="lip_…" />
        <EButton type="submit" :loading="busy" :disabled="!token.trim()">Connect</EButton>
      </form>
    </template>

    <ENotice v-if="error" tone="danger" title="That did not work" :role="false">{{ error }}</ENotice>

    <EDialog
      v-model:open="upgrading"
      title="Make this a bot account?"
      description="This is permanent. A bot account can never play as a person again, and nothing here can undo it."
      size="sm"
    >
      <template #actions>
        <EButton variant="danger" :loading="busy" @click="upgrade">Make it a bot, for good</EButton>
        <EButton variant="quiet" @click="upgrading = false">Cancel</EButton>
      </template>
    </EDialog>
  </div>
</template>

<style scoped>
.setup {
  display: grid;
  align-content: start;
  gap: var(--e-gap-loose);
  max-inline-size: 34rem;
  padding: var(--e-gap-section) var(--ws-pad);
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

.steps {
  display: grid;
  gap: var(--e-gap-base);
  margin: 0;
  padding-inline-start: 1.25rem;
}

.steps li {
  padding-inline-start: var(--e-gap-tight);
}

.form {
  display: grid;
  justify-items: start;
  gap: var(--e-gap-base);
}

.form :deep(.e-field) {
  inline-size: 100%;
}

.row {
  display: flex;
  flex-wrap: wrap;
  gap: var(--e-gap-snug);
}
</style>
