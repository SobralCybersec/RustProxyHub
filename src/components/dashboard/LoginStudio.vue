<script setup lang="ts">
import type { ProviderName, ProviderOverview } from '@/lib/types'

defineProps<{
  providers: ProviderOverview[]
}>()

const store = useStore()

const guides: Record<ProviderName, string[]> = {
  qwen: [
    'Open a global login for the default profile when you need fresh cookies.',
    'Use the Qwen account bank below for per-account persistent profiles.',
    'Mark the session done after the browser state is saved.',
  ],
  deepseek: [
    'Open browser login and finish sign-in on chat.deepseek.com.',
    'Wait until the chat box is usable.',
    'Mark the session done and rerun a hub smoke request.',
  ],
  kimi: [
    'Open browser login and complete sign-in on kimi.com.',
    'Leave the session long enough for the persistent profile to settle.',
    'Mark the session done when the page is ready.',
  ],
  chatgpt: [
    'Open browser login and authenticate on chatgpt.com.',
    'After login, mark the session done so the next model probe can switch headless again.',
    'Live model IDs appear on the provider card after a successful probe.',
  ],
  gemini: [
    'Open browser login and authenticate on gemini.google.com.',
    'Mark the session done after the browser state is saved.',
    'Live model IDs appear on the provider card after a successful probe.',
  ],
}

function loginOpen(provider: ProviderName) {
  return store.overview?.open_provider_login_sessions.includes(provider) ?? false
}
</script>

<template>
  <section class="panel login-panel">
    <div class="panel-top">
      <div>
        <p class="eyebrow">Ritual access</p>
        <h2>Playwright handoff points</h2>
        <p class="panel-copy">
          Visible browser sessions stay last-resort. Open one when auth expires, let cookies settle, then hand
          control back to headless bridge path.
        </p>
      </div>
      <span class="status-chip" data-state="accent">{{ store.openLoginCount }} active</span>
    </div>

    <div class="login-grid">
      <article v-for="provider in providers" :key="provider.name" class="login-card">
        <div class="dossier-index">ACCESS {{ provider.name.toUpperCase() }}</div>

        <div class="panel-top">
          <div>
            <p class="eyebrow">{{ provider.name }}</p>
            <h3>{{ store.browserPrefs[provider.name] }}</h3>
            <p class="panel-copy">Default browser channel for this provider login session.</p>
          </div>
          <span class="status-chip" :data-state="loginOpen(provider.name) ? 'healthy' : 'idle'">
            {{ loginOpen(provider.name) ? 'window open' : 'idle' }}
          </span>
        </div>

        <label class="field">
          <span>Browser</span>
          <select v-model="store.browserPrefs[provider.name]">
            <option value="msedge">msedge</option>
            <option value="chrome">chrome</option>
            <option value="chromium">chromium</option>
          </select>
        </label>

        <div class="login-steps">
          <p v-for="step in guides[provider.name]" :key="step" class="step-line">{{ step }}</p>
        </div>

        <div class="action-row">
          <button
            class="ghost-button"
            :disabled="store.isBusy(`login:start:${provider.name}`)"
            @click="store.startProviderLogin(provider.name)"
          >
            Open ritual
          </button>
          <button
            class="secondary-button"
            :disabled="!loginOpen(provider.name)"
            @click="store.stopProviderLogin(provider.name)"
          >
            Mark done
          </button>
        </div>
      </article>
    </div>
  </section>
</template>
