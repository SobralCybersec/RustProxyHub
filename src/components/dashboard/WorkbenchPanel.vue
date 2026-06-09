<script setup lang="ts">
const store = useStore()
const { hubModelOptions, overview } = storeToRefs(store)
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
      <label class="field span-field">
        <span>Model</span>
        <input v-model="store.workbenchModel" list="hub-model-options" placeholder="qwen:model-id or chatgpt:model-id" />
        <datalist id="hub-model-options">
          <option v-for="model in hubModelOptions" :key="model" :value="model" />
        </datalist>
      </label>

      <label class="field toggle-field">
        <span>Web search</span>
        <input v-model="store.workbenchWebSearch" type="checkbox" />
      </label>

      <label class="field span-field">
        <span>Prompt</span>
        <textarea
          v-model="store.workbenchPrompt"
          rows="7"
          placeholder="Ask for a smoke response and confirm which provider answered."
        />
      </label>

      <div class="action-row">
        <button class="primary-button" :disabled="store.isBusy('workbench:run')" @click="store.runWorkbench()">
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
