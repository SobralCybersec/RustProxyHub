<script setup lang="ts">
const store = useStore()

const details = computed(() => store.activeProviderDetails)
const logs = computed(() => store.activeProviderLogs)

function prettyJson(value: unknown) {
  return JSON.stringify(value, null, 2)
}
</script>

<template>
  <aside v-if="store.activeDrawer" class="drawer-backdrop" @click.self="store.closeProviderDrawer()">
    <section class="drawer-panel">
      <div class="panel-top">
        <div>
          <p class="eyebrow">{{ store.activeDrawer }}</p>
          <h2>Provider details</h2>
          <p class="panel-copy">On-demand health payloads and local runtime notes only. Heavy polling stays off the hot path.</p>
        </div>
        <button class="secondary-button" @click="store.closeProviderDrawer()">Close</button>
      </div>

      <div class="drawer-stack">
        <div class="info-card">
          <p class="info-label">Overview</p>
          <pre class="code-window">{{ prettyJson(details?.overview ?? {}) }}</pre>
        </div>

        <div class="info-card">
          <p class="info-label">Health detail</p>
          <pre class="code-window">{{ prettyJson(details?.detail ?? {}) }}</pre>
        </div>

        <div class="info-card">
          <p class="info-label">Runtime logs</p>
          <pre class="log-window">{{ logs.length ? logs.join('\n') : 'No local lifecycle logs yet.' }}</pre>
        </div>
      </div>
    </section>
  </aside>
</template>
