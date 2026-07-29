<script setup lang="ts">
import { HugeiconsIcon } from '@hugeicons/vue'
import { Globe02Icon, Loading03Icon, SentIcon } from '@hugeicons/core-free-icons'
import { computed, onMounted } from 'vue'
import { useStore } from '@/store'

const store = useStore()
const t = (key: string, params?: Record<string, string | number>) => store.t(key, params)

const busy = computed(() => store.isBusy('workbench:run'))
const hasModels = computed(() => store.hubModelOptions.length > 0)
const hubRunning = computed(() => store.overview?.hub.running ?? false)

// surface the first runtime issue, fall back to plain English
const runtimeHint = computed(
  () => store.runtimeIssues[0] ?? 'Runtime preflight pending — fix issues before running.',
)

onMounted(() => store.syncWorkbenchModel())

function runWorkbench() {
  void store.runWorkbench()
}
</script>

<template>
  <div class="card stack">
    <!-- Model -->
    <div>
      <label class="field-label">{{ t('workbench.model') }}</label>
      <select v-model="store.workbenchModel" class="select">
        <!-- placeholder when no models are available yet -->
        <option v-if="!hasModels" value="" disabled>—</option>
        <option v-for="model in store.hubModelOptions" :key="model" :value="model">
          {{ model }}
        </option>
      </select>
      <span v-if="!hasModels" class="faint models-hint">
        Start the hub and providers to load models
      </span>
    </div>

    <!-- Prompt -->
    <div>
      <label class="field-label">{{ t('workbench.prompt') }}</label>
      <textarea
        v-model="store.workbenchPrompt"
        class="textarea"
        :placeholder="t('workbench.promptPlaceholder')"
        rows="4"
      />
    </div>

    <!-- Web search toggle -->
    <label class="row web-search-row">
      <input v-model="store.workbenchWebSearch" type="checkbox" class="ws-check" />
      <HugeiconsIcon :icon="Globe02Icon" :size="16" class="muted" aria-hidden="true" />
      <span class="muted">{{ t('workbench.webSearch') }}</span>
    </label>

    <!-- Guard hints — not hard blocks; store validates before firing -->
    <div v-if="!store.runtimeReady" class="banner banner-error">
      {{ runtimeHint }}
    </div>
    <p v-else-if="!hubRunning" class="faint hint-text">
      Start the hub first to send a request.
    </p>

    <!-- Run -->
    <div class="row">
      <button class="btn btn-primary" :disabled="busy" @click="runWorkbench">
        <HugeiconsIcon
          :icon="busy ? Loading03Icon : SentIcon"
          :size="16"
          aria-hidden="true"
        />
        {{ busy ? t('workbench.buttons.running') : t('workbench.buttons.run') }}
      </button>
      <span class="spacer" />
    </div>

    <!-- Response — workbenchResponse is already pretty-printed JSON -->
    <div v-if="store.workbenchResponse" class="log-frame">
      <pre class="mono">{{ store.workbenchResponse }}</pre>
    </div>
  </div>
</template>

<style scoped>
.models-hint {
  display: block;
  font-size: 13px;
  margin-top: 4px;
}

.web-search-row {
  cursor: pointer;
  width: fit-content;
}

.ws-check {
  accent-color: var(--primary-focus);
}

.hint-text {
  font-size: 13px;
  margin: 0;
}

/* ponytail: log-frame already sets white-space: pre-wrap; pre inside inherits */
.log-frame pre {
  margin: 0;
}
</style>
