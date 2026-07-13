<script setup lang="ts">
import { HugeiconsIcon } from '@hugeicons/vue'
import {
  CheckmarkBadge01Icon,
  Globe02Icon,
  Login03Icon,
  Logout03Icon,
  MetaIcon,
  SecurityCheckIcon,
} from '@hugeicons/core-free-icons'
import { computed } from 'vue'
import { messageAt } from '@/lib/ui-i18n'
import { burstFromElement, playTone } from '@/effects'
import type { ProviderName, ProviderOverview } from '@/lib/types'
import { useStore } from '@/store'

defineProps<{ providers: ProviderOverview[] }>()
const store = useStore()
const t = (key: string, params?: Record<string, string | number>) => store.t(key, params)

const browsers = ['msedge', 'chrome', 'firefox', 'brave']

const openSessions = computed(() => store.overview?.open_provider_login_sessions ?? [])
function isOpen(name: ProviderName) {
  return openSessions.value.includes(name)
}
function guides(name: ProviderName): string[] {
  const lines = messageAt(store.locale, `login.guides.${name}`)
  return Array.isArray(lines) ? lines.map(String) : []
}
function setBrowser(name: ProviderName, value: string) {
  store.browserPrefs[name] = value
}
function providerIcon(name: ProviderName) {
  return name === 'meta' ? MetaIcon : Globe02Icon
}

function onOpen(event: MouseEvent, name: ProviderName) {
  burstFromElement(event.currentTarget as Element, '#5ce1e6', 12)
  playTone('accent')
  void store.startProviderLogin(name)
}
function onDone(name: ProviderName) {
  playTone('success')
  void store.stopProviderLogin(name)
}
</script>

<template>
  <div class="login-panel">
    <header class="panel-head">
      <div>
        <span class="eyebrow"><HugeiconsIcon :icon="SecurityCheckIcon" :size="12" aria-hidden="true" /> {{ t('common.providers') }}</span>
        <h2 class="panel-title">{{ t('app.headerTabs.access') }}</h2>
      </div>
      <div class="session-counter" :class="{ active: openSessions.length }">
        <span class="counter-num">{{ openSessions.length }}</span>
        <span class="counter-label">{{ t('login.open') }}</span>
      </div>
    </header>

    <div class="login-grid">
      <article v-for="p in providers" :key="p.name" v-tilt class="card login-card" :class="{ open: isOpen(p.name) }">
        <header class="login-card-head">
          <div class="login-id">
            <span class="login-avatar"><HugeiconsIcon :icon="providerIcon(p.name)" :size="18" aria-hidden="true" /></span>
            <div>
              <h3 class="login-name">{{ p.name }}</h3>
              <span class="login-state" :class="p.login_state">
                {{ isOpen(p.name) ? t('login.open') : p.login_state === 'authenticated' ? t('providers.status.ok') : t('login.idle') }}
              </span>
            </div>
          </div>
          <HugeiconsIcon
            v-if="p.login_state === 'authenticated'"
            :icon="CheckmarkBadge01Icon"
            :size="18"
            class="auth-badge"
            aria-hidden="true"
          />
        </header>

        <ol class="login-guides">
          <li v-for="(g, i) in guides(p.name)" :key="i">{{ g }}</li>
        </ol>

        <div class="browser-row">
          <span class="browser-label">{{ t('common.browser') }}</span>
          <div class="browser-pills">
            <button
              v-for="b in browsers"
              :key="b"
              type="button"
              class="browser-pill"
              :class="{ active: store.browserPrefs[p.name] === b }"
              @click="setBrowser(p.name, b)"
            >
              {{ b }}
            </button>
          </div>
        </div>

        <footer class="login-actions">
          <button
            v-if="!isOpen(p.name)"
            type="button"
            class="la-btn primary"
            :disabled="store.isBusy(`login:start:${p.name}`)"
            @click="onOpen($event, p.name)"
          >
            <HugeiconsIcon :icon="Login03Icon" :size="13" aria-hidden="true" />
            {{ p.login_state === 'authenticated' ? t('login.reopenSession') : t('login.openSession') }}
          </button>
          <button
            v-else
            type="button"
            class="la-btn done"
            :disabled="store.isBusy(`login:stop:${p.name}`)"
            @click="onDone(p.name)"
          >
            <HugeiconsIcon :icon="Logout03Icon" :size="13" aria-hidden="true" />
            {{ t('login.markDone') }}
          </button>
        </footer>
      </article>
    </div>
  </div>
</template>

<style scoped>
.login-panel {
  display: flex;
  flex-direction: column;
  gap: 1rem;
  padding: 1.2rem;
}
.panel-head {
  display: flex;
  align-items: flex-start;
  justify-content: space-between;
  gap: 1rem;
}
.eyebrow {
  display: inline-flex;
  align-items: center;
  gap: 0.3rem;
  font-family: var(--font-arcade);
  font-size: 0.5rem;
  color: var(--orange);
  letter-spacing: 0.1em;
  text-transform: uppercase;
}
.panel-title {
  font-family: var(--font-arcade);
  font-size: 0.88rem;
  margin-top: 0.5rem;
  color: var(--text);
}
.session-counter {
  display: flex;
  flex-direction: column;
  align-items: center;
  border: 1px solid var(--line);
  border-radius: var(--radius-sm);
  padding: 0.45rem 0.8rem;
  background: var(--void-2);
}
.session-counter.active {
  border-color: rgba(92, 225, 230, 0.45);
  box-shadow: inset 0 0 12px rgba(92, 225, 230, 0.12);
}
.counter-num {
  font-family: var(--font-arcade);
  font-size: 1rem;
  color: var(--cyan);
}
.counter-label {
  font-size: 0.52rem;
  color: var(--muted);
  text-transform: uppercase;
}

.login-grid {
  display: grid;
  grid-template-columns: repeat(auto-fill, minmax(17rem, 1fr));
  gap: 0.8rem;
}
.login-card {
  padding: 0.95rem;
  display: flex;
  flex-direction: column;
  gap: 0.7rem;
}
.login-card.open {
  border-color: rgba(92, 225, 230, 0.45);
  box-shadow: var(--glow-cyan);
}
.login-card-head {
  display: flex;
  align-items: center;
  justify-content: space-between;
}
.login-id {
  display: flex;
  align-items: center;
  gap: 0.55rem;
}
.login-avatar {
  display: grid;
  place-items: center;
  width: 2rem;
  height: 2rem;
  border: 1px solid var(--line-strong);
  border-radius: var(--radius-sm);
  color: var(--cyan);
  background: var(--void-2);
}
.login-name {
  font-size: 0.82rem;
  text-transform: uppercase;
  letter-spacing: 0.05em;
  color: var(--text);
}
.login-state {
  font-size: 0.56rem;
  text-transform: uppercase;
  letter-spacing: 0.05em;
  color: var(--muted);
}
.login-state.authenticated {
  color: var(--cyan);
}
.auth-badge {
  color: var(--cyan);
  filter: drop-shadow(0 0 4px rgba(92, 225, 230, 0.6));
}

.login-guides {
  list-style: none;
  display: flex;
  flex-direction: column;
  gap: 0.35rem;
  counter-reset: step;
  margin: 0;
  padding: 0;
}
.login-guides li {
  position: relative;
  padding-left: 1.5rem;
  font-size: 0.66rem;
  color: var(--muted-strong);
  line-height: 1.45;
}
.login-guides li::before {
  counter-increment: step;
  content: counter(step);
  position: absolute;
  left: 0;
  top: 0;
  width: 1.05rem;
  height: 1.05rem;
  display: grid;
  place-items: center;
  font-family: var(--font-arcade);
  font-size: 0.42rem;
  color: var(--orange);
  border: 1px solid var(--line-strong);
  border-radius: 2px;
}

.browser-row {
  display: flex;
  flex-direction: column;
  gap: 0.4rem;
}
.browser-label {
  font-size: 0.56rem;
  color: var(--muted);
  text-transform: uppercase;
  letter-spacing: 0.06em;
}
.browser-pills {
  display: flex;
  gap: 0.3rem;
  flex-wrap: wrap;
}
.browser-pill {
  border: 1px solid var(--line);
  background: rgba(255, 255, 255, 0.02);
  color: var(--muted);
  padding: 0.32rem 0.5rem;
  cursor: pointer;
  font-size: 0.56rem;
  text-transform: uppercase;
  border-radius: 2px;
  transition: all 0.15s ease;
}
.browser-pill.active {
  color: var(--orange);
  border-color: var(--orange);
  background: var(--orange-soft);
}

.login-actions {
  margin-top: auto;
}
.la-btn {
  width: 100%;
  display: inline-flex;
  align-items: center;
  justify-content: center;
  gap: 0.4rem;
  border: 1px solid var(--line-strong);
  background: rgba(255, 255, 255, 0.02);
  color: var(--text);
  padding: 0.55rem;
  cursor: pointer;
  font-size: 0.62rem;
  text-transform: uppercase;
  letter-spacing: 0.05em;
  border-radius: var(--radius-sm);
  transition: all 0.15s ease;
}
.la-btn.primary {
  border-color: var(--cyan);
  color: var(--cyan);
  background: var(--cyan-soft);
}
.la-btn.primary:hover:not(:disabled) {
  box-shadow: var(--glow-cyan);
}
.la-btn.done {
  border-color: var(--orange);
  color: var(--orange);
  background: var(--orange-soft);
}
.la-btn.done:hover:not(:disabled) {
  box-shadow: var(--glow-orange);
}
.la-btn:disabled {
  opacity: 0.5;
  cursor: not-allowed;
}

@media (max-width: 520px) {
  .login-grid {
    grid-template-columns: 1fr;
  }
}
</style>
