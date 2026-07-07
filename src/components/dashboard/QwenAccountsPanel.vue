<script setup lang="ts">
import { HugeiconsIcon } from '@hugeicons/vue'
import {
  CheckmarkBadge01Icon,
  Delete02Icon,
  Key01Icon,
  Login03Icon,
  Logout03Icon,
  Mail01Icon,
  UserAdd01Icon,
  UserCircleIcon,
} from '@hugeicons/core-free-icons'
import { computed, ref } from 'vue'
import { burstFromElement, playTone } from '@/effects'
import { useStore } from '@/store'

const store = useStore()
const t = (key: string, params?: Record<string, string | number>) => store.t(key, params)

const accounts = computed(() => store.filteredQwenAccounts)
const openLogins = computed(() => store.overview?.open_qwen_account_login_sessions ?? [])
// ponytail: password lives only in this component ref, cleared on submit; never
// enters the global Pinia store so devtools/other components can't read it.
const qwenPassword = ref('')
function isOpen(id: string) {
  return openLogins.value.includes(id)
}

async function onAdd(event: MouseEvent) {
  burstFromElement(event.currentTarget as Element, '#ff9500', 14)
  playTone('success')
  const password = qwenPassword.value
  try {
    await store.addQwenAccount(password)
  } finally {
    qwenPassword.value = ''
  }
}
function onLogin(id: string) {
  playTone('accent')
  void store.startQwenAccountLogin(id)
}
function onDone(id: string) {
  playTone('success')
  void store.stopQwenAccountLogin(id)
}
function onRemove(id: string) {
  playTone('error')
  void store.removeQwenAccount(id)
}
</script>

<template>
  <div class="qwen-panel">
    <header class="panel-head">
      <div>
        <span class="eyebrow"><HugeiconsIcon :icon="UserCircleIcon" :size="12" aria-hidden="true" /> {{ t('common.accounts') }}</span>
        <h2 class="panel-title">{{ t('app.headerTabs.qwen') }}</h2>
      </div>
      <div class="account-counter">
        <span class="counter-num">{{ accounts.length }}</span>
        <span class="counter-label">{{ t('common.accounts') }}</span>
      </div>
    </header>

    <form class="add-form card" @submit.prevent>
      <div class="field">
        <span class="field-icon"><HugeiconsIcon :icon="Mail01Icon" :size="15" aria-hidden="true" /></span>
        <input
          v-model="store.qwenEmail"
          type="email"
          class="field-input"
          :placeholder="t('common.email')"
          autocomplete="off"
        />
      </div>
      <div class="field">
        <span class="field-icon"><HugeiconsIcon :icon="Key01Icon" :size="15" aria-hidden="true" /></span>
        <input
          v-model="qwenPassword"
          type="password"
          class="field-input"
          :placeholder="t('qwen.passwordPlaceholder')"
          autocomplete="off"
        />
      </div>
      <button type="submit" class="add-btn" :disabled="store.isBusy('qwen-account:add')" @click="onAdd">
        <HugeiconsIcon :icon="UserAdd01Icon" :size="14" aria-hidden="true" />
        {{ t('qwen.saveAccount') }}
      </button>
    </form>

    <div v-if="!accounts.length" class="empty-state">{{ t('qwen.empty') }}</div>

    <ul v-else class="account-list">
      <li v-for="a in accounts" :key="a.id" v-tilt class="card account-row" :class="{ open: isOpen(a.id) }">
        <span class="account-avatar"><HugeiconsIcon :icon="UserCircleIcon" :size="22" aria-hidden="true" /></span>
        <div class="account-info">
          <span class="account-email">{{ a.email }}</span>
          <span class="account-meta">
            <span class="account-id">{{ a.id }}</span>
            <span v-if="a.created_at" class="account-date">· {{ a.created_at }}</span>
            <span class="account-tag" :class="{ on: a.has_password }">
              <HugeiconsIcon v-if="a.has_password" :icon="CheckmarkBadge01Icon" :size="11" aria-hidden="true" />
              {{ a.has_password ? t('qwen.passwordSet') : t('qwen.manualOnly') }}
            </span>
          </span>
        </div>
        <div class="account-actions">
          <span v-if="isOpen(a.id)" class="open-badge">{{ t('qwen.profileOpen') }}</span>
          <button
            type="button"
            class="qa-btn"
            :disabled="isOpen(a.id) || store.isBusy(`login:qwen-account:start:${a.id}`)"
            @click="onLogin(a.id)"
          >
            <HugeiconsIcon :icon="Login03Icon" :size="13" aria-hidden="true" />
            {{ t('qwen.openProfile') }}
          </button>
          <button
            v-if="isOpen(a.id)"
            type="button"
            class="qa-btn done"
            :disabled="store.isBusy(`login:qwen-account:stop:${a.id}`)"
            @click="onDone(a.id)"
          >
            <HugeiconsIcon :icon="Logout03Icon" :size="13" aria-hidden="true" />
            {{ t('qwen.complete') }}
          </button>
          <button
            type="button"
            class="qa-btn danger"
            :disabled="store.isBusy(`qwen-account:remove:${a.id}`)"
            @click="onRemove(a.id)"
          >
            <HugeiconsIcon :icon="Delete02Icon" :size="13" aria-hidden="true" />
            {{ t('qwen.remove') }}
          </button>
        </div>
      </li>
    </ul>
  </div>
</template>

<style scoped>
.qwen-panel {
  display: flex;
  flex-direction: column;
  gap: 1rem;
  padding: 1.2rem;
}
.panel-head {
  display: flex;
  align-items: flex-start;
  justify-content: space-between;
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
.account-counter {
  display: flex;
  flex-direction: column;
  align-items: center;
  border: 1px solid var(--line);
  border-radius: var(--radius-sm);
  padding: 0.45rem 0.8rem;
  background: var(--void-2);
}
.counter-num {
  font-family: var(--font-arcade);
  font-size: 1rem;
  color: var(--orange);
  text-shadow: var(--glow-orange);
}
.counter-label {
  font-size: 0.52rem;
  color: var(--muted);
  text-transform: uppercase;
}

.add-form {
  display: flex;
  gap: 0.5rem;
  padding: 0.7rem;
  flex-wrap: wrap;
}
.field {
  position: relative;
  flex: 1;
  min-width: 11rem;
  display: flex;
  align-items: center;
}
.field-icon {
  position: absolute;
  left: 0.55rem;
  color: var(--muted);
  display: grid;
  place-items: center;
}
.field-input {
  width: 100%;
  border: 1px solid var(--line);
  background: var(--void-2);
  color: var(--text);
  padding: 0.55rem 0.6rem 0.55rem 2rem;
  font-size: 0.72rem;
  border-radius: var(--radius-sm);
  font-family: var(--font-mono);
}
.field-input:focus {
  outline: none;
  border-color: var(--orange);
  box-shadow: inset 0 0 0 1px var(--orange-soft);
}
.add-btn {
  display: inline-flex;
  align-items: center;
  gap: 0.4rem;
  border: 1px solid var(--orange);
  background: var(--orange);
  color: var(--void);
  padding: 0.55rem 0.9rem;
  cursor: pointer;
  font-size: 0.64rem;
  font-weight: 700;
  text-transform: uppercase;
  letter-spacing: 0.05em;
  border-radius: var(--radius-sm);
  transition: all 0.15s ease;
}
.add-btn:hover:not(:disabled) {
  box-shadow: var(--glow-orange);
}
.add-btn:disabled {
  opacity: 0.5;
  cursor: not-allowed;
}

.empty-state {
  border: 1px dashed var(--line-strong);
  border-radius: var(--radius-md);
  padding: 2.5rem 1rem;
  text-align: center;
  color: var(--muted);
  font-size: 0.78rem;
}

.account-list {
  list-style: none;
  display: flex;
  flex-direction: column;
  gap: 0.55rem;
  margin: 0;
  padding: 0;
}
.account-row {
  display: flex;
  align-items: center;
  gap: 0.7rem;
  padding: 0.7rem 0.85rem;
}
.account-row.open {
  border-color: rgba(92, 225, 230, 0.45);
  box-shadow: var(--glow-cyan);
}
.account-avatar {
  display: grid;
  place-items: center;
  width: 2.3rem;
  height: 2.3rem;
  border: 1px solid var(--line-strong);
  border-radius: var(--radius-sm);
  color: var(--orange);
  background: var(--void-2);
  flex-shrink: 0;
}
.account-info {
  display: flex;
  flex-direction: column;
  gap: 0.2rem;
  min-width: 0;
  flex: 1;
}
.account-email {
  font-size: 0.76rem;
  color: var(--text);
  font-weight: 600;
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
}
.account-meta {
  display: flex;
  align-items: center;
  gap: 0.45rem;
  flex-wrap: wrap;
  font-size: 0.58rem;
  color: var(--muted);
}
.account-id {
  color: var(--cyan);
}
.account-tag {
  display: inline-flex;
  align-items: center;
  gap: 0.25rem;
  border: 1px solid var(--line);
  border-radius: 2px;
  padding: 0.12rem 0.35rem;
  text-transform: uppercase;
  letter-spacing: 0.04em;
}
.account-tag.on {
  color: var(--cyan);
  border-color: rgba(92, 225, 230, 0.4);
}
.account-actions {
  display: flex;
  gap: 0.35rem;
  flex-shrink: 0;
}
.qa-btn {
  display: inline-flex;
  align-items: center;
  gap: 0.3rem;
  border: 1px solid var(--line);
  background: rgba(255, 255, 255, 0.02);
  color: var(--muted-strong);
  padding: 0.42rem 0.55rem;
  cursor: pointer;
  font-size: 0.58rem;
  text-transform: uppercase;
  letter-spacing: 0.04em;
  border-radius: var(--radius-sm);
  transition: all 0.15s ease;
}
.qa-btn:hover:not(:disabled) {
  color: var(--cyan);
  border-color: var(--cyan);
  background: var(--cyan-soft);
}
.qa-btn.done {
  color: var(--orange);
  border-color: var(--orange);
  background: var(--orange-soft);
}
.qa-btn.danger:hover:not(:disabled) {
  color: var(--danger);
  border-color: var(--danger);
  background: var(--danger-soft);
}
.qa-btn:disabled {
  opacity: 0.5;
  cursor: not-allowed;
}

@media (max-width: 560px) {
  .account-row {
    flex-wrap: wrap;
  }
  .account-actions {
    width: 100%;
    justify-content: flex-end;
  }
}
</style>
