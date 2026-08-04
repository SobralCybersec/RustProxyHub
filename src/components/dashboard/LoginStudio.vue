<script setup lang="ts">
import { computed } from 'vue'
import { HugeiconsIcon } from '@hugeicons/vue'
import { Login03Icon, Logout03Icon, BrowserIcon, LinkSquare01Icon, Copy01Icon } from '@hugeicons/core-free-icons'
import type { ProviderName } from '@/lib/types'
import { useStore } from '@/store'

const store = useStore()
const t = (key: string) => store.t(key)

const hub = computed(() => store.overview?.hub ?? null)
const providers = computed(() => store.overview?.providers ?? [])
const openSessions = computed(() => store.overview?.open_provider_login_sessions ?? [])

function isOpen(name: ProviderName): boolean {
  return openSessions.value.includes(name)
}

// authenticated → green, needs_login/expired → amber, everything else → idle grey
function dotClass(loginState: string): string {
  if (loginState === 'authenticated') return 'dot ok'
  if (loginState === 'needs_login' || loginState === 'expired') return 'dot warn'
  return 'dot'
}

function setBrowser(name: ProviderName, value: string) {
  store.browserPrefs[name] = value
}

function copyHubUrl() {
  const url = hub.value?.base_url
  if (url) void navigator.clipboard.writeText(url)
}

function openLogin(name: ProviderName) {
  void store.startProviderLogin(name)
}

function closeLogin(name: ProviderName) {
  void store.stopProviderLogin(name)
}
</script>

<template>
  <div class="stack">
    <!-- ── Hub access ── -->
    <div class="card stack">
      <div class="row">
        <span class="strong">Hub access</span>
        <div class="spacer" />
        <span v-if="hub" class="chip">
          API key: {{ hub.api_key_enabled ? t('common.enabled') : t('common.disabled') }}
        </span>
      </div>

      <template v-if="hub">
        <div class="row">
          <span class="field-label muted">Base URL</span>
          <span class="mono spacer url-text">{{ hub.base_url }}</span>
          <button class="btn btn-utility" type="button" @click="copyHubUrl">
            <HugeiconsIcon :icon="Copy01Icon" :size="16" aria-hidden="true" />
            {{ t('common.copy') }}
          </button>
        </div>

        <div class="row">
          <span class="field-label muted">OpenAPI</span>
          <span class="mono spacer url-text">{{ hub.openapi_url }}</span>
          <a :href="hub.openapi_url" target="_blank" rel="noopener" class="btn btn-utility">
            <HugeiconsIcon :icon="LinkSquare01Icon" :size="16" aria-hidden="true" />
            Open docs
          </a>
        </div>
      </template>

      <p v-else class="faint">Waiting for hub data…</p>
    </div>

    <!-- ── Provider login sessions ── -->
    <div v-for="p in providers" :key="p.name" class="card row">
      <!-- status dot + name + state label -->
      <span :class="dotClass(p.login_state)" />
      <span class="strong">{{ p.name }}</span>
      <span class="muted">{{ p.login_state }}</span>

      <div class="spacer" />

      <!-- browser picker -->
      <div class="row browser-row">
        <HugeiconsIcon :icon="BrowserIcon" :size="16" class="faint" aria-hidden="true" />
        <select
          class="select browser-sel"
          :value="store.browserPrefs[p.name]"
          @change="setBrowser(p.name, ($event.target as HTMLSelectElement).value)"
        >
          <option value="msedge">msedge</option>
          <option value="chrome">chrome</option>
          <option value="chromium">chromium</option>
        </select>
      </div>

      <!-- session action -->
      <button
        v-if="isOpen(p.name)"
        type="button"
        class="btn btn-utility"
        :disabled="store.isBusy(`login:stop:${p.name}`)"
        @click="closeLogin(p.name)"
      >
        <HugeiconsIcon :icon="Logout03Icon" :size="16" aria-hidden="true" />
        Close session
      </button>
      <button
        v-else
        type="button"
        class="btn btn-primary"
        :disabled="store.isBusy(`login:start:${p.name}`)"
        @click="openLogin(p.name)"
      >
        <HugeiconsIcon :icon="Login03Icon" :size="16" aria-hidden="true" />
        {{ t('login.openSession') }}
      </button>
    </div>

    <!-- ── Workbench block notice ── -->
    <div v-if="store.openLoginCount > 0" class="banner banner-notice">
      {{ store.openLoginCount }} open browser {{ store.openLoginCount === 1 ? 'session' : 'sessions' }} — the workbench
      is blocked until all login windows are closed.
    </div>
  </div>
</template>

<style scoped>
/* ponytail: only layout overrides the design system can't express inline */
.url-text {
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
  max-width: 320px;
}

.browser-sel {
  width: auto;
  min-width: 100px;
}

/* keep the provider row's select from expanding to 100% (design system default) */
.browser-row {
  gap: 6px;
}
</style>
