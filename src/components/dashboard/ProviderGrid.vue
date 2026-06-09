<script setup lang="ts">
import type { ProviderName, ProviderOverview } from '@/lib/types'

defineProps<{
  providers: ProviderOverview[]
}>()

const store = useStore()

const providerTitles: Record<ProviderName, string> = {
  qwen: 'Qwen account bank',
  deepseek: 'DeepSeek bridge',
  kimi: 'Kimi auto-continue',
  chatgpt: 'ChatGPT browser session',
  gemini: 'Gemini browser session',
}

const providerNotes: Record<ProviderName, string> = {
  qwen: 'Rotation, uploads, stop control, and prefixed hub models.',
  deepseek: 'Reasoning-heavy browser proxy with normalized search flag support.',
  kimi: 'Pause-aware browser proxy with explicit unsupported-search warnings.',
  chatgpt: 'Manual Playwright login, live model discovery, and bridged completions.',
  gemini: 'Manual Playwright login, live model discovery, and bridged completions.',
}

function loginOpen(provider: ProviderName) {
  return store.overview?.open_provider_login_sessions.includes(provider) ?? false
}

function statusTone(provider: ProviderOverview) {
  if (!provider.running) return 'idle'
  if (provider.login_state === 'authenticated') return 'healthy'
  if (provider.health_status === 'ok') return 'running'
  if (provider.health_status === 'degraded') return 'degraded'
  return 'running'
}

function formatStarted(value: number | null) {
  if (!value) return 'n/a'
  return new Date(value * 1000).toLocaleString()
}
</script>

<template>
  <section class="panel providers-panel">
    <div class="panel-top">
      <div>
        <p class="eyebrow">Provider dossiers</p>
        <h2>Embedded proxy surfaces</h2>
        <p class="panel-copy">
          Each bridge lives inside same runtime spine now. Use cards for fast triage, then open full dossier
          when you need health payloads or local logs.
        </p>
      </div>
      <span class="status-chip" :data-state="providers.length ? 'accent' : 'idle'">
        {{ providers.length }} visible
      </span>
    </div>

    <div class="provider-grid provider-grid-expanded">
      <article v-for="provider in providers" :key="provider.name" class="provider-panel">
        <div class="dossier-index">FILE {{ provider.name.toUpperCase() }}</div>

        <div class="panel-top">
          <div>
            <p class="eyebrow">{{ provider.name }}</p>
            <h3>{{ providerTitles[provider.name] }}</h3>
            <p class="panel-copy">{{ providerNotes[provider.name] }}</p>
          </div>
          <span class="status-chip" :data-state="statusTone(provider)">
            {{ provider.login_state.replaceAll('_', ' ') }}
          </span>
        </div>

        <dl class="facts">
          <div>
            <dt>Health</dt>
            <dd>{{ provider.health_status }}</dd>
          </div>
          <div>
            <dt>Models</dt>
            <dd>{{ provider.model_count }}</dd>
          </div>
          <div>
            <dt>Started</dt>
            <dd>{{ formatStarted(provider.started_at) }}</dd>
          </div>
        </dl>

        <div class="info-card">
          <p class="info-label">Base URL</p>
          <p class="mono-line">{{ provider.base_url }}</p>
        </div>

        <div class="provider-meta-line">
          <span class="mini-pill" :data-state="provider.web_search_supported ? 'healthy' : 'idle'">
            {{ provider.web_search_supported ? 'web search mapped' : 'web search warned' }}
          </span>
          <span v-if="provider.last_error" class="mini-pill" data-state="degraded">last error captured</span>
          <span v-if="loginOpen(provider.name)" class="mini-pill" data-state="accent">login window open</span>
        </div>

        <div class="model-cloud">
          <span v-for="model in provider.models.slice(0, 8)" :key="`${provider.name}:${model}`" class="model-chip">
            {{ provider.name }}:{{ model }}
          </span>
          <span v-if="!provider.models.length" class="empty-chip">No live models loaded yet.</span>
        </div>

        <div class="action-row">
          <button
            class="ghost-button"
            :disabled="store.isBusy(`login:start:${provider.name}`)"
            @click="store.startProviderLogin(provider.name)"
          >
            {{ loginOpen(provider.name) ? 'Reopen login' : 'Open login' }}
          </button>
          <button
            class="secondary-button"
            :disabled="!loginOpen(provider.name)"
            @click="store.stopProviderLogin(provider.name)"
          >
            Mark done
          </button>
          <button class="primary-button" @click="store.openProviderDrawer(provider.name)">Open dossier</button>
        </div>
      </article>
    </div>
  </section>
</template>
