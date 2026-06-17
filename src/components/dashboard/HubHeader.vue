<script setup lang="ts">
import { computed, ref } from 'vue'
import type { DashboardOverview } from '@/lib/types'
import { useStore } from '@/store'

const props = defineProps<{ overview: DashboardOverview | null }>()
const store = useStore()

const searchValue = computed({
  get: () => store.searchQuery,
  set: (value: string) => store.setSearchQuery(value),
})
const hub = computed(() => props.overview?.hub ?? null)
const copied = ref<string | null>(null)
const isReady = computed(() => props.overview?.runtime.single_runner_ready ?? false)
const issues = computed(() => props.overview?.runtime.issues ?? [])
const startupMode = computed(() => props.overview?.startup_config?.mode ?? 'manual')
const startupServices = computed(() => props.overview?.startup_config?.services ?? [])
const hubRunning = computed(() => props.overview?.hub.running ?? false)
const stats = computed(() => [
  ['hub.status', hub.value?.health_status ?? 'booting'],
  ['hub.running', hub.value?.running ? 'true' : 'false'],
  ['hub.models', hub.value?.model_count ?? 0],
  ['qwen.accounts', props.overview?.qwen_account_count ?? 0],
  ['login.windows', store.openLoginCount],
  ['runner.ready', isReady.value ? 'true' : 'false'],
])

function copyText(value: string | null | undefined, label: string) {
  if (!value) return
  void navigator.clipboard?.writeText(value)
  copied.value = label
  setTimeout(() => (copied.value = null), 1600)
}
</script>

<template>
  <header class="terminal-panel header-terminal">
    <div class="terminal-line"><span class="prompt">visitor@rustproxy:~$</span> ./overview.sh --watch --rect</div>

    <div class="overview-grid">
      <section class="terminal-box hero-box">
        <p class="eyebrow">BOOT BUFFER</p>
        <h1>RustProxy Terminal Control</h1>
        <p class="copy">
          Main page is now the terminal. Each area below behaves like a rectangular shell pane instead of a soft dashboard card.
        </p>

        <div class="runtime-row" :class="isReady ? 'ok' : 'fail'">
          <span>{{ isReady ? '[ OK ]' : '[ FAIL ]' }}</span>
          <strong>{{ isReady ? 'single-runner preflight passed' : 'single-runner preflight blocked' }}</strong>
        </div>

        <ul v-if="issues.length" class="issue-list">
          <li v-for="issue in issues" :key="issue">stderr: {{ issue }}</li>
        </ul>

        <label class="terminal-field search-field">
          <span>grep providers</span>
          <input v-model="searchValue" type="search" placeholder="qwen, models, errors, login state..." />
        </label>

        <div class="actions">
          <button :disabled="store.isRefreshing" @click.stop="store.refreshOverview()">
            {{ store.isRefreshing ? 'refreshing...' : './refresh.sh' }}
          </button>
          <button :disabled="hubRunning || store.isBusy('service:start:hub')" @click.stop="store.startService('hub')">
            {{ hubRunning ? 'hub online' : (store.isBusy('service:start:hub') ? 'starting hub...' : 'start hub') }}
          </button>
          <button :disabled="!hub?.base_url" @click.stop="copyText(hub?.base_url, 'url')">
            {{ copied === 'url' ? 'copied' : 'copy hub url' }}
          </button>
          <button :disabled="!hub?.openapi_url" @click.stop="copyText(hub?.openapi_url, 'api')">
            {{ copied === 'api' ? 'copied' : 'copy openapi' }}
          </button>
        </div>
      </section>

      <aside class="terminal-box stats-box">
        <div class="file-title">/proc/rustproxy/status</div>
        <div v-for="([label, value], i) in stats" :key="label" class="stat-row">
          <span>{{ String(i + 1).padStart(2, '0') }}</span>
          <strong>{{ label }}</strong>
          <code>{{ value }}</code>
        </div>
        <pre class="mini-log">{{ JSON.stringify({ base_url: hub?.base_url, openapi_url: hub?.openapi_url, running: hub?.running, startup_mode: startupMode, startup_services: startupServices }, null, 2) }}</pre>
      </aside>
    </div>
  </header>
</template>

<style scoped>
/* hard square terminal reset */
*,
*::before,
*::after {
  border-radius: 0 !important;
}

:deep(*),
:deep(*::before),
:deep(*::after) {
  border-radius: 0 !important;
}

input, textarea, select { user-select: text; }

.header-terminal {
  width: 100%;
  padding: 1rem;
  color: lightgreen;
}
.terminal-line {
  margin-bottom: .8rem;
  font-size: 1.2rem;
}
.prompt { color: #7fff9a; margin-right: .45rem; }
.overview-grid {
  display: grid;
  grid-template-columns: minmax(0, 1.35fr) minmax(18rem, .65fr);
  gap: .8rem;
}
.terminal-box {
  border: 1px solid rgba(144, 238, 144, .42);
  background: #020702;
  padding: .85rem;
}
.eyebrow {
  margin: 0 0 .35rem;
  color: #7fff9a;
  letter-spacing: .14em;
  text-transform: uppercase;
}
h1 {
  margin: 0;
  color: #dfffe4;
  font-size: clamp(2rem, 4vw, 3.4rem);
  line-height: .92;
  font-weight: 400;
}
.copy {
  max-width: 58rem;
  color: rgba(196, 255, 202, .78);
  margin: .7rem 0 1rem;
}
.runtime-row {
  display: flex;
  gap: .6rem;
  align-items: center;
  border: 1px solid rgba(144, 238, 144, .32);
  padding: .5rem .65rem;
  margin-bottom: .7rem;
  background: rgba(144, 238, 144, .08);
}
.runtime-row.fail {
  border-color: rgba(255, 91, 91, .45);
  color: #ffc1c1;
  background: rgba(80, 0, 0, .22);
}
.issue-list {
  margin: .5rem 0;
  padding-left: 1.2rem;
  color: #ffb3b3;
}
.terminal-field {
  display: grid;
  gap: .35rem;
  color: #7fff9a;
  text-transform: uppercase;
  letter-spacing: .1em;
}
input {
  border: 1px solid rgba(144, 238, 144, .42);
  background: #000;
  color: lightgreen;
  padding: .65rem;
  font: inherit;
  outline: none;
}
.actions {
  display: flex;
  flex-wrap: wrap;
  gap: .45rem;
  margin-top: .7rem;
}
button {
  border: 1px solid rgba(144, 238, 144, .42);
  background: #041004;
  color: lightgreen;
  padding: .55rem .75rem;
  cursor: pointer;
  font: inherit;
}
button:hover { background: rgba(144, 238, 144, .16); }
button:disabled { opacity: .42; cursor: not-allowed; }
.file-title {
  border-bottom: 1px solid rgba(144, 238, 144, .35);
  padding-bottom: .45rem;
  margin-bottom: .45rem;
  color: #dfffe4;
}
.stat-row {
  display: grid;
  grid-template-columns: 2rem minmax(7rem, 1fr) auto;
  gap: .65rem;
  padding: .32rem 0;
  border-bottom: 1px dashed rgba(144, 238, 144, .18);
}
.stat-row span { color: rgba(144, 238, 144, .65); }
.stat-row code { color: #dfffe4; }
.mini-log {
  margin: .8rem 0 0;
  padding: .65rem;
  border: 1px solid rgba(144, 238, 144, .24);
  background: #000;
  color: rgba(196, 255, 202, .8);
  white-space: pre-wrap;
  word-break: break-word;
}
@media (max-width: 880px) {
  .overview-grid { grid-template-columns: 1fr; }
}
</style>
