<script setup lang="ts">
import { computed } from 'vue'
import type { DashboardOverview } from '@/lib/types'

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
      <p class="eyebrow">RustProxyHub Desktop</p>
      <h1>One EXE. One hub. Every browser-backed proxy under one roof.</h1>
      <p class="lede">
        The app now owns the embedded providers, the OpenAI-compatible hub, and the login flows. Search,
        inspect, log in, and run smoke prompts from one control room.
      </p>

      <label class="field search-field">
        <span>Search everything</span>
        <input v-model="searchValue" type="search" placeholder="providers, models, accounts, log states" />
      </label>
    </div>

    <div class="hero-rail">
      <div class="hero-actions">
        <button class="primary-button" :disabled="store.isRefreshing" @click="store.refreshOverview()">
          {{ store.isRefreshing ? 'Refreshing...' : 'Refresh hub pulse' }}
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
      </div>
    </div>
  </header>
</template>
