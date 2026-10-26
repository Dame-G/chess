<script setup lang="ts">
import { computed, ref, watch } from "vue";
import { useQueryCache } from "@pinia/colada";
import { useRouter } from "vue-router";
import { EButton } from "@eos-ui/vue/components/EButton";
import { ECheckbox } from "@eos-ui/vue/components/ECheckbox";
import { EDialog } from "@eos-ui/vue/components/EDialog";
import { ENotice } from "@eos-ui/vue/components/ENotice";
import { ETextField } from "@eos-ui/vue/components/ETextField";
import { api } from "../../api/runner";
import { unwrap, type Version } from "../../api/queries";
import { liveBuilds } from "./versions";

/** Deleting a branch cannot be undone, so the name has to be typed again. */
const props = defineProps<{ version: Version }>();
const open = defineModel<boolean>("open", { default: false });

const router = useRouter();
const cache = useQueryCache();
const typed = ref("");
const remote = ref(false);
const builds = ref(false);
const busy = ref(false);
const error = ref("");

watch(open, (now) => {
  if (!now) return;
  typed.value = "";
  remote.value = false;
  builds.value = false;
  error.value = "";
});

const count = computed(() => liveBuilds(props.version).length);
const sure = computed(() => typed.value === props.version.name);

async function remove() {
  if (!sure.value) return;
  busy.value = true;
  error.value = "";
  try {
    await unwrap(
      api.DELETE("/api/versions/{name}", {
        params: { path: { name: props.version.name }, query: { remote: remote.value, builds: builds.value } },
        body: { confirm: typed.value },
      }),
    );
    open.value = false;
    void cache.invalidateQueries({ key: ["versions"] });
    void cache.invalidateQueries({ key: ["branches"] });
    void cache.invalidateQueries({ key: ["builds"] });
    await router.replace("/versions");
  } catch (e) {
    error.value = (e as Error).message;
  } finally {
    busy.value = false;
  }
}
</script>

<template>
  <EDialog v-model:open="open" role="alertdialog" :title="`Delete the branch ${version.name}?`" size="md">
    <form id="delete-version" class="body" @submit.prevent="remove">
      <p class="lede">
        The branch is removed from this computer. Tests and tunes that used it keep their records, and so do its builds
        unless you delete them too.
      </p>
      <ECheckbox
        v-model="remote"
        label="Also delete it on GitHub"
        description="Removes the branch from origin as well."
      />
      <ECheckbox
        v-model="builds"
        :disabled="!count"
        :label="`Also delete its builds`"
        :description="
          count
            ? `Frees the ${count === 1 ? 'binary' : count + ' binaries'} of ${version.name}.`
            : 'It has no builds on disk.'
        "
      />
      <ETextField
        v-model="typed"
        :label="`Type ${version.name} to confirm`"
        autocomplete="off"
        spellcheck="false"
        data-autofocus
      />
      <ENotice v-if="error" tone="danger" title="It was not deleted" :role="false">{{ error }}</ENotice>
    </form>
    <template #actions>
      <EButton type="submit" form="delete-version" variant="danger" :disabled="!sure" :loading="busy">
        Delete branch
      </EButton>
      <EButton variant="outline" @click="open = false">Keep it</EButton>
    </template>
  </EDialog>
</template>

<style scoped>
.body {
  display: grid;
  gap: var(--e-gap-base);
}

.lede {
  margin: 0;
}
</style>
