<script setup lang="ts">
const store = useStore()
const { hubModelOptions, overview } = storeToRefs(store)
</script>

<template>
  <section class="panel workbench-panel">
    <div class="panel-top">
      <div>
        <p class="eyebrow">Unified workbench</p>
        <h2>Real hub request</h2>
        <p class="panel-copy">
          Requests always go through the embedded hub now. Pick a provider-prefixed model and decide whether the
          normalized web-search flag should be sent.
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
          {{ store.isBusy('workbench:run') ? 'Running...' : 'Run hub request' }}
        </button>
      </div>
    </div>

    <pre class="code-window large">{{ store.workbenchResponse || 'The live JSON response lands here.' }}</pre>
  </section>
</template>
