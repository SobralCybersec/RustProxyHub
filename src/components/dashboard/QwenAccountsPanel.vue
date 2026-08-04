<script setup lang="ts">
import { ref, computed } from 'vue'
import { storeToRefs } from 'pinia'
import { HugeiconsIcon } from '@hugeicons/vue'
import { UserAdd01Icon, Delete02Icon, Login03Icon, Logout03Icon, Mail01Icon } from '@hugeicons/core-free-icons'
import { useStore } from '@/store'

const store = useStore()
const { qwenEmail, browserPrefs, filteredQwenAccounts, overview } = storeToRefs(store)

// ponytail: password lives only here — never enters Pinia; cleared on every submit.
const localPassword = ref('')

const openSessions = computed(() => overview.value?.open_qwen_account_login_sessions ?? [])

function isLoginOpen(id: string) {
  return openSessions.value.includes(id)
}

async function onAddAccount() {
  const password = localPassword.value
  await store.addQwenAccount(password)
  localPassword.value = ''
}
</script>

<template>
  <div class="panel-root stack">
    <!-- Header -->
    <div class="row">
      <h2 class="strong">{{ store.t('common.accounts') }}</h2>
      <span class="chip">{{ filteredQwenAccounts.length }}</span>
    </div>

    <!-- Add-account form -->
    <div class="card stack">
      <div class="field-group">
        <label class="field-label" for="qwen-email">
          <HugeiconsIcon :icon="Mail01Icon" :size="12" aria-hidden="true" />
          {{ store.t('common.email') }}
        </label>
        <input
          id="qwen-email"
          v-model="qwenEmail"
          type="email"
          class="input"
          placeholder="account@example.com"
          autocomplete="off"
        />
      </div>

      <div class="field-group">
        <label class="field-label" for="qwen-password">{{ store.t('common.password') }}</label>
        <input
          id="qwen-password"
          v-model="localPassword"
          type="password"
          class="input"
          :placeholder="store.t('qwen.passwordPlaceholder')"
          autocomplete="new-password"
        />
      </div>

      <div class="field-group">
        <label class="field-label" for="qwen-browser">{{ store.t('common.browser') }}</label>
        <select id="qwen-browser" v-model="browserPrefs.qwen" class="select">
          <option value="msedge">Microsoft Edge</option>
          <option value="chrome">Chrome</option>
          <option value="chromium">Chromium</option>
        </select>
      </div>

      <div>
        <button
          type="button"
          class="btn btn-primary"
          :disabled="store.isBusy('qwen-account:add')"
          @click="onAddAccount"
        >
          <HugeiconsIcon :icon="UserAdd01Icon" :size="16" aria-hidden="true" />
          {{ store.t('qwen.saveAccount') }}
        </button>
      </div>
    </div>

    <!-- Empty state -->
    <div v-if="!filteredQwenAccounts.length" class="card empty-state">
      <span class="muted">{{ store.t('qwen.empty') }}</span>
    </div>

    <!-- Accounts list -->
    <div v-else class="stack">
      <div v-for="account in filteredQwenAccounts" :key="account.id" class="card account-card stack">
        <!-- Identity row -->
        <div class="row">
          <div class="account-identity">
            <span class="strong account-email">{{ account.email }}</span>
            <span class="mono faint account-id">{{ account.id }}</span>
          </div>
          <div class="spacer" />
          <span v-if="account.has_password" class="chip">
            <span class="dot ok" />
            {{ store.t('qwen.passwordSet') }}
          </span>
        </div>

        <!-- Meta + actions row -->
        <div class="row">
          <span v-if="account.created_at" class="faint date-label">{{ account.created_at }}</span>
          <div class="spacer" />

          <!-- Login toggle -->
          <button
            v-if="isLoginOpen(account.id)"
            type="button"
            class="btn btn-utility"
            :disabled="store.isBusy(`login:qwen-account:stop:${account.id}`)"
            @click="store.stopQwenAccountLogin(account.id)"
          >
            <HugeiconsIcon :icon="Logout03Icon" :size="16" aria-hidden="true" />
            Close login
          </button>
          <button
            v-else
            type="button"
            class="btn btn-utility"
            :disabled="store.isBusy(`login:qwen-account:start:${account.id}`)"
            @click="store.startQwenAccountLogin(account.id)"
          >
            <HugeiconsIcon :icon="Login03Icon" :size="16" aria-hidden="true" />
            Open login
          </button>

          <!-- Remove -->
          <button
            type="button"
            class="btn btn-danger"
            :disabled="store.isBusy(`qwen-account:remove:${account.id}`)"
            @click="store.removeQwenAccount(account.id)"
          >
            <HugeiconsIcon :icon="Delete02Icon" :size="16" aria-hidden="true" />
            {{ store.t('qwen.remove') }}
          </button>
        </div>
      </div>
    </div>
  </div>
</template>

<style scoped>
.panel-root {
  padding: var(--lg);
}

.field-group {
  display: flex;
  flex-direction: column;
}

.account-card {
  gap: var(--sm);
}

.account-identity {
  display: flex;
  flex-direction: column;
  gap: 2px;
  min-width: 0;
}

.account-email {
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
  font-size: 15px;
}

.account-id {
  font-size: 11px;
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
}

.date-label {
  font-size: 13px;
}

.empty-state {
  text-align: center;
  padding: var(--xl) var(--lg);
}
</style>
