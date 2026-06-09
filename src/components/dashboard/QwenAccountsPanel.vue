<script setup lang="ts">
const store = useStore()

function sessionOpen(accountId: string) {
  return store.overview?.open_qwen_account_login_sessions.includes(accountId) ?? false
}
</script>

<template>
  <section class="panel account-panel">
    <div class="panel-top">
      <div>
        <p class="eyebrow">Qwen vault</p>
        <h2>Stored rotation accounts</h2>
        <p class="panel-copy">
          Qwen account rotation stays advanced-only. Seed identity once, then reopen exact persistent profile when
          traffic needs per-account isolation.
        </p>
      </div>
      <span class="status-chip" data-state="accent">{{ store.qwenAccounts.length }} stored</span>
    </div>

    <form class="account-form" @submit.prevent="store.addQwenAccount()">
      <label class="field">
        <span>Email</span>
        <input v-model="store.qwenEmail" type="email" placeholder="operator@domain.com" />
      </label>
      <label class="field">
        <span>Password</span>
        <input v-model="store.qwenPassword" type="password" placeholder="optional for seeded login" />
      </label>
      <div class="action-row">
        <button class="primary-button" type="submit" :disabled="store.isBusy('qwen-account:add')">
          {{ store.isBusy('qwen-account:add') ? 'Saving...' : 'Save account' }}
        </button>
      </div>
    </form>

    <div class="account-list">
      <article v-for="account in store.filteredQwenAccounts" :key="account.id" class="account-card">
        <div class="dossier-index">ACCOUNT</div>

        <div class="panel-top">
          <div>
            <p class="eyebrow">{{ account.id }}</p>
            <h3>{{ account.email }}</h3>
            <p class="panel-copy">
              {{ account.has_password ? 'Password seeded' : 'Manual browser login only' }}
            </p>
          </div>
          <span class="status-chip" :data-state="sessionOpen(account.id) ? 'healthy' : 'idle'">
            {{ sessionOpen(account.id) ? 'profile open' : 'saved' }}
          </span>
        </div>

        <div class="action-row">
          <button
            class="ghost-button"
            :disabled="store.isBusy(`login:qwen-account:start:${account.id}`)"
            @click="store.startQwenAccountLogin(account.id)"
          >
            Open profile
          </button>
          <button
            class="secondary-button"
            :disabled="!sessionOpen(account.id)"
            @click="store.stopQwenAccountLogin(account.id)"
          >
            Mark done
          </button>
          <button
            class="danger-button"
            :disabled="store.isBusy(`qwen-account:remove:${account.id}`)"
            @click="store.removeQwenAccount(account.id)"
          >
            Remove
          </button>
        </div>
      </article>

      <div v-if="!store.filteredQwenAccounts.length" class="info-card">
        <p class="info-label">Account bank</p>
        <p class="mono-line">No accounts match current trace filter.</p>
      </div>
    </div>
  </section>
</template>
