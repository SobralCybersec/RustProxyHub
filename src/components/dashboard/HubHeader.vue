<script setup lang="ts">
import { computed } from 'vue'
import type { DashboardOverview } from '@/lib/types'
import { useStore } from '@/store'

const props = defineProps<{
  overview: DashboardOverview | null
}>()

const store = useStore()

const searchValue = computed({
  get: () => store.searchQuery,
  set: (value: string) => store.setSearchQuery(value),
})

const hub = computed(() => props.overview?.hub ?? null)

function copyText(value: string | null | undefined) {
  if (!value) return
  void navigator.clipboard?.writeText(value)
}
</script>

<template>
  <header class="hero panel">
    <div class="hero-copy">
      <p class="eyebrow">RustProxy Control Room</p>
      <h1>One desktop shell. Five browser-backed providers. One hub watching all of them.</h1>
      <p class="lede">
        Runtime moved out of Tauri edge work and into one internal engine room. Track status, reopen manual
        sessions, inspect health, and run live hub probes without leaving this dossier wall.
      </p>

      <label class="field search-field">
        <span>Trace filter</span>
        <input v-model="searchValue" type="search" placeholder="providers, models, accounts, states, errors" />
      </label>
    </div>

    <div class="hero-rail">
      <div class="hero-warning">
        <span class="warning-mark">Runtime status</span>
        <p v-if="overview?.runtime.single_runner_ready">
          Single-runner preflight passed. Bundled helper, bundled <code>node.exe</code>, writable data dir, and Edge check all look good.
        </p>
        <p v-else>
          Single-runner preflight blocked startup. Fix listed runtime issues before trusting provider status.
        </p>
      </div>

      <div class="hero-actions">
        <button class="primary-button" :disabled="store.isRefreshing" @click="store.refreshOverview()">
          {{ store.isRefreshing ? 'Refreshing...' : 'Refresh surveillance' }}
        </button>
        <button class="ghost-button" :disabled="!hub?.base_url" @click="copyText(hub?.base_url)">
          Copy hub URL
        </button>
        <button class="ghost-button" :disabled="!hub?.openapi_url" @click="copyText(hub?.openapi_url)">
          Copy OpenAPI
        </button>
      </div>

      <div class="stat-stack">
        <div class="stat-pill">
          <span>Hub state</span>
          <strong>{{ hub?.health_status ?? 'booting' }}</strong>
        </div>
        <div class="stat-pill">
          <span>Hub models</span>
          <strong>{{ hub?.model_count ?? 0 }}</strong>
        </div>
        <div class="stat-pill">
          <span>Qwen bank</span>
          <strong>{{ overview?.qwen_account_count ?? 0 }}</strong>
        </div>
        <div class="stat-pill">
          <span>Login windows</span>
          <strong>{{ store.openLoginCount }}</strong>
        </div>
        <div class="stat-pill">
          <span>Runner</span>
          <strong>{{ overview?.runtime.single_runner_ready ? 'ready' : 'degraded' }}</strong>
        </div>
      </div>

      <div class="hub-ribbon">
        <div class="info-card">
          <p class="info-label">Hub base URL</p>
          <p class="mono-line">{{ hub?.base_url ?? 'booting embedded hub...' }}</p>
        </div>
        <div class="info-card">
          <p class="info-label">OpenAPI</p>
          <p class="mono-line">{{ hub?.openapi_url ?? 'waiting for /openapi.json' }}</p>
        </div>
        <div class="info-card">
          <p class="info-label">Helper root</p>
          <p class="mono-line">{{ overview?.runtime.helper_dir ?? overview?.helper_dir ?? 'resolving helper assets...' }}</p>
        </div>
        <div class="info-card">
          <p class="info-label">Bundled Node</p>
          <p class="mono-line">{{ overview?.runtime.node_path ?? 'node.exe missing from bundle' }}</p>
        </div>
        <div class="info-card">
          <p class="info-label">Node source</p>
          <p class="mono-line">{{ overview?.runtime.node_source ?? 'unresolved' }}</p>
        </div>
        <div class="info-card">
          <p class="info-label">Runtime data</p>
          <p class="mono-line">{{ overview?.app_data_dir ?? 'resolving app data dir...' }}</p>
        </div>
      </div>

      <div v-if="overview?.runtime.issues.length" class="hub-ribbon issues-ribbon">
        <div v-for="issue in overview.runtime.issues" :key="issue" class="info-card issue-card">
          <p class="info-label">Runtime issue</p>
          <p>{{ issue }}</p>
        </div>
      </div>
    </div>
  </header>
</template>
