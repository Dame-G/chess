<script setup lang="ts">
import { ref } from "vue";
import { useQueryCache } from "@pinia/colada";
import { PhCheckCircle, PhMinusCircle, PhTrash, PhWarning } from "@phosphor-icons/vue";
import { EButton } from "@eos-ui/vue/components/EButton";
import { EDialog } from "@eos-ui/vue/components/EDialog";
import { EIcon } from "@eos-ui/vue/components/EIcon";
import { ENotice } from "@eos-ui/vue/components/ENotice";
import { ETag } from "@eos-ui/vue/components/ETag";
import { ETable } from "@eos-ui/vue/components/ETable";
import { EVisuallyHidden } from "@eos-ui/vue/components/EVisuallyHidden";
import { api } from "../../api/runner";
import { unwrap, useExperiments, type Version, type VersionBuild } from "../../api/queries";
import { ago, speed } from "../../format";
import { checkOf, type Check } from "./versions";

/** The builds of one version, newest first: a detail, for checking a fingerprint or freeing a binary. */
const props = defineProps<{ version: Version }>();

const marks: Record<Check, { icon: typeof PhCheckCircle; label: string; class: string }> = {
  match: { icon: PhCheckCircle, label: "Matches its commit's Bench line", class: "ok" },
  differs: { icon: PhWarning, label: "Differs from its commit's Bench line", class: "bad" },
  unchecked: { icon: PhMinusCircle, label: "The commit has no Bench line to compare with", class: "none" },
};

const { state: tests } = useExperiments();

/** Why a build cannot be deleted right now, when that is already known: a short tag shown in its row. */
function reason(b: VersionBuild): string {
  if (!b.on_disk) return "";
  if (props.version.on_lichess && b.id === props.version.latest_build_id) return "On Lichess";
  const using = tests.value.data?.find(
    (e) => (e.status === "queued" || e.status === "running") && (e.dev_build === b.id || e.base_build === b.id),
  );
  return using ? `In use by test ${using.id}` : "";
}

const cache = useQueryCache();
const target = ref<VersionBuild>();
const open = ref(false);
const busy = ref(false);
const error = ref("");

function ask(b: VersionBuild) {
  target.value = b;
  error.value = "";
  open.value = true;
}

async function remove() {
  if (!target.value) return;
  busy.value = true;
  error.value = "";
  try {
    await unwrap(api.DELETE("/api/builds/{id}", { params: { path: { id: target.value.id } } }));
    open.value = false;
    void cache.invalidateQueries({ key: ["versions"] });
    void cache.invalidateQueries({ key: ["builds"] });
  } catch (e) {
    // 409 carries the reason: a running test, tune or match uses the build.
    const message = (e as Error).message;
    error.value = /^409\b/.test(message)
      ? "A running test, tune or match uses this build, or the bot plays it."
      : message;
  } finally {
    busy.value = false;
  }
}
</script>

<template>
  <ETable caption="Builds" hide-caption dense cards>
    <thead>
      <tr>
        <th scope="col">Build</th>
        <th scope="col">Commit</th>
        <th scope="col" class="e-num">Fingerprint</th>
        <th scope="col" class="e-num">Speed</th>
        <th scope="col">Built</th>
        <th scope="col"><EVisuallyHidden>Actions</EVisuallyHidden></th>
      </tr>
    </thead>
    <tbody>
      <tr v-for="b in version.builds" :key="b.id" :class="{ gone: !b.on_disk }">
        <th scope="row" class="num">
          #{{ b.id }}<span v-if="b.spsa" class="note"> tuning</span><span v-if="!b.on_disk" class="note"> deleted</span>
        </th>
        <td data-label="Commit" class="num">{{ b.commit.slice(0, 7) }}</td>
        <td data-label="Fingerprint" class="e-num num">
          <span class="bench">
            {{ b.bench.toLocaleString("en") }}
            <span :class="['mark', marks[checkOf(b)].class]" :title="marks[checkOf(b)].label">
              <EIcon :name="marks[checkOf(b)].icon" :label="marks[checkOf(b)].label" />
            </span>
          </span>
        </td>
        <td data-label="Speed" class="e-num num">{{ speed(b.nps) }}</td>
        <td data-label="Built" :title="new Date(b.created_at).toLocaleString('en')">{{ ago(b.created_at) }}</td>
        <td class="act">
          <template v-if="b.on_disk">
            <ETag v-if="reason(b)" size="sm" :label="reason(b)" />
            <EButton
              v-else
              variant="quiet"
              size="sm"
              :icon="PhTrash"
              :title="`Delete the binary of build #${b.id}`"
              :aria-label="`Delete build #${b.id}`"
              @click="ask(b)"
            >
              Delete
            </EButton>
          </template>
        </td>
      </tr>
    </tbody>
  </ETable>

  <EDialog v-model:open="open" role="alertdialog" :title="`Delete build #${target?.id}?`" size="sm">
    Its binary is removed from this computer. Tests, tunes and matches that used it still show it, and building the
    version again makes a new build.
    <ENotice v-if="error" class="fail" tone="danger" title="It was not deleted" :role="false">{{ error }}</ENotice>
    <template #actions>
      <EButton variant="danger" :loading="busy" @click="remove">Delete build</EButton>
      <EButton variant="outline" @click="open = false">Keep it</EButton>
    </template>
  </EDialog>
</template>

<style scoped>
.gone {
  color: var(--e-muted);
}

.note {
  color: var(--e-muted);
  font-weight: 400;
}

.bench {
  display: inline-flex;
  align-items: center;
  gap: var(--e-gap-snug);
}

.mark {
  display: inline-flex;
}

.ok {
  color: var(--e-success);
}

.bad {
  color: var(--e-danger);
}

.none {
  color: var(--e-muted);
}

.act {
  text-align: end;
}

.fail {
  margin-block-start: var(--e-gap-base);
}
</style>
