<script setup lang="ts">
import { useStore } from '@/store'

const store = useStore()
function sessionOpen(accountId: string) {
  return store.overview?.open_qwen_account_login_sessions.includes(accountId) ?? false
}
</script>

<template>
  <section class="terminal-panel qwen-terminal">
    <div class="terminal-line"><span class="prompt">visitor@rustproxy:~$</span> vault qwen --list --rotation --square</div>

    <form class="account-form" @submit.prevent="store.addQwenAccount()">
      <label>
        <span>email</span>
        <input v-model="store.qwenEmail" type="email" placeholder="operator@domain.com" />
      </label>
      <label>
        <span>password</span>
        <input v-model="store.qwenPassword" type="password" placeholder="optional seeded login" />
      </label>
      <button type="submit" :disabled="store.isBusy('qwen-account:add')">
        {{ store.isBusy('qwen-account:add') ? 'saving…' : 'save account' }}
      </button>
    </form>

    <div class="account-list">
      <article v-for="account in store.filteredQwenAccounts" :key="account.id" class="account-row">
        <div>
          <strong>{{ account.email }}</strong>
          <small>{{ account.id }} · {{ account.has_password ? 'password seeded' : 'manual profile only' }}</small>
        </div>
        <span :class="{ live: sessionOpen(account.id) }">{{ sessionOpen(account.id) ? '[ profile open ]' : '[ saved ]' }}</span>
        <div class="actions">
          <button :disabled="store.isBusy(`login:qwen-account:start:${account.id}`)" @click.stop="store.startQwenAccountLogin(account.id)">open profile</button>
          <button :disabled="!sessionOpen(account.id)" @click.stop="store.stopQwenAccountLogin(account.id)">done</button>
          <button :disabled="store.isBusy(`qwen-account:remove:${account.id}`)" @click.stop="store.removeQwenAccount(account.id)">remove</button>
        </div>
      </article>
      <p v-if="!store.filteredQwenAccounts.length" class="empty">stdout: no qwen accounts match current grep filter.</p>
    </div>
  </section>
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

.qwen-terminal { width: 100%; padding: .9rem; }
.terminal-line { color: #b9ffd0; font-size: 1.2rem; margin-bottom: .75rem; }
.prompt { color: #7fff9a; margin-right: .45rem; }
.account-form { display: grid; grid-template-columns: 1fr 1fr auto; gap: .65rem; margin-bottom: .85rem; align-items: end; }
label { display: grid; gap: .35rem; color: #7fff9a; text-transform: uppercase; letter-spacing: .1em; }
input {
  border: 1px solid rgba(144, 238, 144, .42);
  background: #000;
  color: lightgreen;
  padding: .62rem;
  font: inherit;
  outline: none;
}
button {
  border: 1px solid rgba(144, 238, 144, .42);
  background: #041004;
  color: lightgreen;
  padding: .5rem .65rem;
  cursor: pointer;
  font: inherit;
}
button:hover { background: rgba(144, 238, 144, .16); }
button:disabled { opacity: .42; cursor: not-allowed; }
.account-list { display: grid; gap: .55rem; }
.account-row {
  display: grid;
  grid-template-columns: 1fr auto auto;
  gap: .7rem;
  align-items: center;
  border: 1px solid rgba(144, 238, 144, .32);
  padding: .65rem;
  background: #020702;
}
strong { color: #eafff0; }
small { display: block; color: rgba(196, 255, 202, .65); margin-top: .2rem; }
.account-row > span { color: rgba(196, 255, 202, .65); }
.account-row > span.live { color: lightgreen; }
.actions { display: flex; flex-wrap: wrap; gap: .35rem; }
.empty { color: rgba(196, 255, 202, .75); }
@media (max-width: 860px) {
  .account-form,
  .account-row { grid-template-columns: 1fr; }
}
</style>
