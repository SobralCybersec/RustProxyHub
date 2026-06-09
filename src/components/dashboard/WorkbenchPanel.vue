<script setup lang="ts">
import { storeToRefs } from 'pinia'
import { computed } from 'vue'
import { useStore } from '@/store'

const store = useStore()
const { hubModelOptions, overview } = storeToRefs(store)
const runtimeBlocked = computed(() => (overview.value ? !overview.value.runtime.single_runner_ready : false))
const runtimeIssueText = computed(() => overview.value?.runtime.issues.join(' ') ?? '')
</script>

<template>
  <section class="panel workbench-panel">
    <div class="panel-top">
      <div>
        <p class="eyebrow">Trial terminal</p>
        <h2>Real hub request</h2>
        <p class="panel-copy">
          Requests always pass through embedded hub. Pick provider-prefixed model, toggle normalized search flag,
          then inspect raw JSON reply below.
        </p>
      </div>
      <span class="status-chip" :data-state="overview?.hub.running ? 'healthy' : 'idle'">
        {{ overview?.hub.running ? 'hub live' : 'hub booting' }}
      </span>
    </div>

    <div class="workbench-grid">
      <div v-if="runtimeBlocked" class="panel-alert span-field">
        <strong>Runtime blocked.</strong>
        <p>{{ runtimeIssueText }}</p>
      </div>

      <label class="field span-field">
        <span>Model</span>
        <input
          v-model="store.workbenchModel"
          list="hub-model-options"
          placeholder="qwen:model-id or chatgpt:model-id"
          :disabled="runtimeBlocked"
        />
        <datalist id="hub-model-options">
          <option v-for="model in hubModelOptions" :key="model" :value="model" />
        </datalist>
      </label>

      <label class="field toggle-field">
        <span>Web search</span>
        <input v-model="store.workbenchWebSearch" type="checkbox" :disabled="runtimeBlocked" />
      </label>

      <label class="field span-field">
        <span>Prompt</span>
        <textarea
          v-model="store.workbenchPrompt"
          rows="7"
          placeholder="Ask for a smoke response and confirm which provider answered."
          :disabled="runtimeBlocked"
        />
      </label>

      <div class="action-row">
        <button
          class="primary-button"
          :disabled="store.isBusy('workbench:run') || runtimeBlocked"
          @click="store.runWorkbench()"
        >
          {{ store.isBusy('workbench:run') ? 'Running...' : 'Run live probe' }}
        </button>
      </div>
    </div>

    <div class="terminal-shell">
      <div class="terminal-bar">
        <span>HUB STREAM</span>
        <span>{{ store.workbenchModel || 'no model selected' }}</span>
      </div>
      <pre class="code-window large">{{ store.workbenchResponse || 'The live JSON response lands here.' }}</pre>
    </div>
  </section>
</template>
