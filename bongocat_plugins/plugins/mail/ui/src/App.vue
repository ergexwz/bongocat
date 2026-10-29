<script setup lang="ts">
import { invoke } from '@tauri-apps/api/core'
import { emit, listen } from '@tauri-apps/api/event'
import { computed, nextTick, onMounted, ref } from 'vue'
import { useI18n } from 'vue-i18n'

import Bubble from '@/mail/components/Bubble/index.vue'
import MailItem from '@/mail/components/MailItem/index.vue'
import MailPanel from '@/mail/components/MailPanel/index.vue'
import type { MailAccount, MailNotification, MailSettings } from '@/mail/types'
import { matchProvider, matchProviderLogo, resolveWebmailUrl } from '@/mail/utils/providers'
import { absoluteDate, relativeTime } from '@/mail/utils/timeFormat'
import { currentVisible, hideCurrent, showAt } from '@/window'

const view = new URLSearchParams(window.location.search).get('view') ?? 'runtime'
const { t, locale } = useI18n()

const accounts = ref<MailAccount[]>([])
const notifications = ref<MailNotification[]>([])
const settings = ref<MailSettings>({
  proxy: '',
  bubbleEnabled: true,
  bubbleAutoDismiss: false,
  unreadOnly: false,
})
const visible = ref(false)
const now = ref(Date.now())
const bubbles = ref<Array<any>>([])
const bubbleHeight = ref(120)

const unread = computed(() => notifications.value.filter((item) => item.status === 'unread'))
const read = computed(() => notifications.value.filter((item) => item.status === 'read'))
const archived = computed(() =>
  notifications.value
    .filter((item) => item.status === 'archived')
    .sort((left, right) => (right.archivedAt ?? 0) - (left.archivedAt ?? 0)),
)
const currentBubble = computed(() => bubbles.value[0])

async function hydrate() {
  const snapshot = await invoke<{
    accounts: MailAccount[]
    notifications: MailNotification[]
    settings: MailSettings
  }>('mail_snapshot')
  accounts.value = snapshot.accounts
  notifications.value = snapshot.notifications
  settings.value = snapshot.settings
}

async function show() {
  if (view === 'runtime') return
  visible.value = false
  await nextTick()
  await showAt(
    view as 'mail-list' | 'mail-archive' | 'mail-settings' | 'bubble',
    bubbleHeight.value + 16,
  )
  visible.value = true
}

async function close() {
  visible.value = false
  await new Promise((resolve) => setTimeout(resolve, 200))
  await hideCurrent()
}

function statusLabel(status: string) {
  switch (status) {
    case 'connected': return t('plugins.mail.labels.statusConnected')
    case 'connecting': return t('plugins.mail.labels.statusConnecting')
    case 'error': return t('plugins.mail.labels.statusError')
    default: return t('plugins.mail.labels.statusIdle')
  }
}

function accountFor(accountId: string): MailAccount | undefined {
  return accounts.value.find((account) => account.id === accountId)
}

function providerName(accountId: string): string | null {
  const account = accountFor(accountId)
  return account ? (matchProvider(account.address)?.displayName ?? null) : null
}

function timeLabel(mail: MailNotification) {
  return relativeTime(mail.arrivedAt, now.value)
}

function dateLabel(mail: MailNotification) {
  return absoluteDate(mail.arrivedAt)
}

async function openMail(mail: MailNotification) {
  const account = accountFor(mail.accountId)
  const url = account ? resolveWebmailUrl(account.address, account.imapHost) : null
  await invoke('mail_mark_read', { id: mail.id })
  if (url) {
    await invoke('mail_open_url', { url })
  }
  await hydrate()
}

async function archiveMail(id: string) {
  await invoke('mail_archive', { id })
  await hydrate()
}

async function deleteMail(id: string) {
  await invoke('mail_delete_notification', { id })
  await hydrate()
}

async function bubbleAction() {
  const bubble = currentBubble.value
  if (!bubble) return
  const account = accountFor(bubble.accountId)
  const url = account ? resolveWebmailUrl(account.address, account.imapHost) : null
  await invoke('mail_mark_read_uid', { accountId: bubble.accountId, uid: bubble.uid })
  if (url) {
    await invoke('mail_open_url', { url })
  }
  bubbles.value.shift()
  if (bubbles.value.length) await show()
  else await close()
}

async function bubbleClose() {
  bubbles.value.shift()
  if (bubbles.value.length) await show()
  else await close()
}

async function bubbleResize(height: number) {
  bubbleHeight.value = Math.max(80, Math.round(height))
  if (visible.value && view === 'bubble') {
    await showAt('bubble', bubbleHeight.value + 16)
  }
}

let dismissTimer: ReturnType<typeof setTimeout> | undefined

function addBubble(payload: any) {
  const account = accountFor(payload.accountId)
  const provider = account ? matchProvider(account.address)?.displayName : null
  const item = {
    id: `${payload.accountId}-${payload.uid}-${payload.arrivedAt}`,
    notificationId: payload.notificationId ?? '',
    accountId: payload.accountId,
    uid: payload.uid,
    source: provider
      ? t('plugins.mail.labels.bubbleSourceProvider', { provider })
      : t('plugins.mail.labels.bubbleSourceDefault'),
    title: payload.from,
    subtitle: payload.subject,
  }
  if (bubbles.value.some((bubble) => bubble.id === item.id)) return
  bubbles.value.push(item)
  void show()
  if (settings.value.bubbleAutoDismiss) {
    if (dismissTimer) clearTimeout(dismissTimer)
    dismissTimer = setTimeout(() => void bubbleClose(), 5_000)
  }
}

// ── settings ──
const address = ref('')
const imapHost = ref('')
const imapPort = ref(993)
const username = ref('')
const password = ref('')
const saving = ref(false)
const providerHint = ref('')
const inputLogo = computed(() => matchProviderLogo(address.value))

function onAddressBlur() {
  if (!username.value) username.value = address.value
  const preset = matchProvider(address.value)
  if (preset) {
    imapHost.value = preset.imapHost
    imapPort.value = preset.imapPort
    providerHint.value = t(`plugins.mail.labels.providers.${preset.hintKey}`)
  } else {
    providerHint.value = ''
  }
}

async function testAndSave() {
  if (!address.value || !imapHost.value || !username.value || !password.value) {
    return
  }
  saving.value = true
  try {
    const input = {
      address: address.value,
      imapHost: imapHost.value,
      imapPort: Number(imapPort.value),
      username: username.value,
      password: password.value,
    }
    await invoke('mail_test_connection', {
      input,
      proxy: settings.value.proxy || null,
    })
    await invoke('mail_add_account', {
      input,
      proxy: settings.value.proxy || null,
    })
    address.value = ''
    imapHost.value = ''
    imapPort.value = 993
    username.value = ''
    password.value = ''
    providerHint.value = ''
    await hydrate()
  } catch (error) {
    console.error(error)
  } finally {
    saving.value = false
  }
}

async function toggleAccount(account: MailAccount) {
  await invoke('mail_set_enabled', {
    id: account.id,
    enabled: !account.enabled,
    proxy: settings.value.proxy || null,
  })
  await hydrate()
}

async function removeAccount(account: MailAccount) {
  await invoke('mail_remove_account', { id: account.id })
  await hydrate()
}

async function saveSettings() {
  await invoke('mail_set_settings', { settings: settings.value })
  await hydrate()
}

onMounted(async () => {
  const initialLocale = await invoke<string>('get_locale')
  locale.value = initialLocale
  await hydrate()

  await listen('companion://event', async ({ payload }: { payload: any }) => {
    switch (payload?.type) {
      case 'open':
        if (payload.kind === view) await show()
        break
      case 'state_changed':
        await hydrate()
        break
      case 'locale':
        locale.value = payload.locale
        break
      default:
        break
    }
  })

  if (view === 'runtime') {
    await invoke('mail_reconnect_all')
    await listen('mail://new-mail', async ({ payload }: { payload: any }) => {
      const inserted = await invoke<boolean>('mail_record_new', { payload })
      await hydrate()
      if (inserted && settings.value.bubbleEnabled) {
        await emit('mail://show-bubble', payload)
      }
    })
    await listen('mail://connection-status', async ({ payload }: { payload: any }) => {
      await invoke('mail_record_status', { payload })
      await hydrate()
    })
    await listen('mail://last-seen-uid', async ({ payload }: { payload: any }) => {
      await invoke('mail_record_last_seen', { payload })
    })
    await listen('mail://state', async () => {
      await hydrate()
    })
    return
  }

  await listen('mail://state', async () => {
    await hydrate()
  })

  if (view === 'bubble') {
    await listen('mail://show-bubble', ({ payload }: { payload: any }) => {
      addBubble(payload)
    })
  }

  const pending = await invoke<string | null>('take_pending_open', { view })
  if (pending === view) await show()

  if (view === 'bubble' && (await currentVisible())) {
    await show()
  }

  now.value = Date.now()
  setInterval(() => {
    now.value = Date.now()
  }, 60_000)
})
</script>

<template>
  <div
    v-if="view === 'mail-list' || view === 'mail-archive'"
    class="bubble-handdrawn mail-page fade"
    :class="{ 'fade-shown': visible }"
  >
    <MailPanel>
      <div class="panel-header" data-tauri-drag-region>
        <span class="panel-title">
          {{ view === 'mail-list' ? t('plugins.mail.labels.mailListTitle') : t('plugins.mail.labels.mailArchiveTitle') }}
        </span>
        <button class="panel-close" type="button" @click="close">
          {{ t('plugins.mail.labels.bubbleClose') }}
        </button>
      </div>
      <div class="panel-scroll">
        <template v-if="view === 'mail-list'">
          <div v-if="unread.length" class="section-label">
            {{ t('plugins.mail.labels.mailListUnreadSection') }} · {{ unread.length }}
          </div>
          <MailItem
            v-for="mail in unread"
            :key="mail.id"
            :mail="mail"
            :provider-name="providerName(mail.accountId)"
            :time-label="timeLabel(mail)"
            :date-label="dateLabel(mail)"
            :show-archive="true"
            @action="openMail(mail)"
            @archive="archiveMail(mail.id)"
            @delete="deleteMail(mail.id)"
          />
          <div v-if="read.length" class="section-label">
            {{ t('plugins.mail.labels.mailListReadSection') }} · {{ read.length }}
          </div>
          <MailItem
            v-for="mail in read"
            :key="mail.id"
            :mail="mail"
            :provider-name="providerName(mail.accountId)"
            :time-label="timeLabel(mail)"
            :date-label="dateLabel(mail)"
            :show-archive="true"
            @action="openMail(mail)"
            @archive="archiveMail(mail.id)"
            @delete="deleteMail(mail.id)"
          />
          <div v-if="!unread.length && !read.length" class="empty">
            {{ t('plugins.mail.labels.mailEmpty') }}
          </div>
        </template>
        <template v-else>
          <div v-if="archived.length" class="section-label">
            {{ t('plugins.mail.labels.mailArchiveSection') }} · {{ archived.length }}
          </div>
          <MailItem
            v-for="mail in archived"
            :key="mail.id"
            :mail="mail"
            :status="'archived'"
            :provider-name="providerName(mail.accountId)"
            :time-label="timeLabel(mail)"
            :date-label="dateLabel(mail)"
            @action="openMail(mail)"
            @archive="archiveMail(mail.id)"
            @delete="deleteMail(mail.id)"
          />
          <div v-if="!archived.length" class="empty">
            {{ t('plugins.mail.labels.mailArchiveEmpty') }}
          </div>
        </template>
      </div>
    </MailPanel>
  </div>

  <div
    v-else-if="view === 'mail-settings'"
    class="bubble-handdrawn mail-page fade"
    :class="{ 'fade-shown': visible }"
  >
    <MailPanel>
      <div class="panel-header" data-tauri-drag-region>
        <span class="panel-title">{{ t('plugins.mail.labels.preferenceTitle') }}</span>
        <button class="panel-close" type="button" @click="close">
          {{ t('plugins.mail.labels.bubbleClose') }}
        </button>
      </div>
      <div class="panel-scroll">
        <div class="settings">
        <h3>{{ t('plugins.mail.labels.bindTitle') }}</h3>
        <form @submit.prevent="testAndSave">
          <label>
            {{ t('plugins.mail.labels.address') }}
            <span class="address-row">
              <img :src="inputLogo" alt="" />
              <input v-model="address" :placeholder="t('plugins.mail.labels.addressPlaceholder')" @blur="onAddressBlur" />
            </span>
          </label>
          <label>
            {{ t('plugins.mail.labels.imapHost') }}
            <input v-model="imapHost" :placeholder="t('plugins.mail.labels.imapHostPlaceholder')" />
          </label>
          <label>
            {{ t('plugins.mail.labels.imapPort') }}
            <input v-model.number="imapPort" type="number" min="1" max="65535" />
          </label>
          <label>
            {{ t('plugins.mail.labels.username') }}
            <input v-model="username" :placeholder="t('plugins.mail.labels.usernamePlaceholder')" />
          </label>
          <label>
            {{ t('plugins.mail.labels.password') }}
            <input v-model="password" type="password" :placeholder="t('plugins.mail.labels.passwordPlaceholder')" />
          </label>
          <p class="hint">{{ t('plugins.mail.labels.passwordHint') }}</p>
          <p v-if="providerHint" class="hint pre">{{ providerHint }}</p>
          <button type="submit" :disabled="saving">
            {{ saving ? t('plugins.mail.labels.connecting') : t('plugins.mail.labels.testAndSave') }}
          </button>
        </form>

        <h3>{{ t('plugins.mail.labels.accountsTitle') }}</h3>
        <div v-for="account in accounts" :key="account.id" class="account">
          <div>
            <strong>{{ account.address }}</strong>
            <small>{{ statusLabel(account.status) }}</small>
          </div>
          <button type="button" @click="toggleAccount(account)">
            {{ account.enabled ? t('plugins.mail.labels.statusConnected') : t('plugins.mail.labels.statusIdle') }}
          </button>
          <button type="button" @click="removeAccount(account)">
            {{ t('plugins.mail.labels.removeButton') }}
          </button>
        </div>

        <h3>{{ t('plugins.mail.labels.notificationTitle') }}</h3>
        <label class="check">
          <input v-model="settings.bubbleEnabled" type="checkbox" @change="saveSettings" />
          {{ t('plugins.mail.labels.bubbleEnabledTitle') }}
        </label>
        <label class="check">
          <input v-model="settings.bubbleAutoDismiss" type="checkbox" @change="saveSettings" />
          {{ t('plugins.mail.labels.bubbleAutoDismissTitle') }}
        </label>
        <label class="check">
          <input v-model="settings.unreadOnly" type="checkbox" @change="saveSettings" />
          {{ t('plugins.mail.labels.unreadOnlyTitle') }}
        </label>
        <label>
          {{ t('plugins.mail.labels.proxy') }}
          <input v-model="settings.proxy" :placeholder="t('plugins.mail.labels.proxyPlaceholder')" @change="saveSettings" />
        </label>
          <p class="hint">{{ t('plugins.mail.labels.proxyHint') }}</p>
        </div>
      </div>
    </MailPanel>
  </div>

  <div
    v-else-if="view === 'bubble'"
    class="bubble-handdrawn bubble-root fade"
    :class="{ 'fade-shown': visible }"
  >
    <Bubble
      v-if="currentBubble"
      type="mail"
      :source="currentBubble.source"
      :title="currentBubble.title"
      :subtitle="currentBubble.subtitle"
      :width="360"
      @action="bubbleAction"
      @close="bubbleClose"
      @resize="bubbleResize"
    />
  </div>
</template>

<style>
html,
body,
#app {
  background: transparent !important;
  margin: 0;
  width: 100%;
  height: 100%;
  overflow: hidden;
  color: var(--ink);
  font-family: 'Handwriting851', -apple-system, 'PingFang SC', sans-serif;
}

* {
  box-sizing: border-box;
}

.mail-page {
  width: 100vw;
  height: 100vh;
  padding: 12px;
  overflow: hidden;
}

.bubble-root {
  width: 100vw;
  height: 100vh;
  overflow: hidden;
  display: flex;
  justify-content: center;
}

.fade {
  opacity: 0;
  transition: opacity 200ms ease;
}

.fade.fade-shown {
  opacity: 1;
}

.panel-header {
  display: flex;
  align-items: center;
  justify-content: space-between;
  margin-bottom: 16px;
  cursor: grab;
}

.panel-header:active {
  cursor: grabbing;
}

.panel-title {
  font-size: 22px;
  font-weight: 700;
  pointer-events: none;
}

.panel-close {
  border: 0;
  background: transparent;
  color: var(--ink-soft);
  font: inherit;
  cursor: pointer;
}

.panel-scroll {
  flex: 1 1 0;
  min-height: 0;
  overflow-x: hidden;
  overflow-y: auto;
  scrollbar-color: var(--ink-faint) transparent;
  scrollbar-width: thin;
}

.panel-scroll::-webkit-scrollbar {
  width: 8px;
}

.panel-scroll::-webkit-scrollbar-track {
  background: transparent;
}

.panel-scroll::-webkit-scrollbar-thumb {
  border-radius: 999px;
  background: var(--ink-faint);
}

.section-label {
  margin: 12px 0 4px;
  color: var(--ink-soft);
  font-size: 15px;
  font-weight: 700;
}

.empty {
  padding: 32px 0;
  text-align: center;
  color: var(--ink-faint);
}

.settings {
  display: flex;
  min-width: 0;
  flex-direction: column;
  gap: 10px;
  padding: 0 6px 18px 0;
  font-family: -apple-system, 'PingFang SC', sans-serif;
  font-size: 13px;
}

.settings h3 {
  margin: 10px 0 0;
  font-size: 15px;
}

.settings form {
  display: flex;
  min-width: 0;
  flex-direction: column;
  gap: 8px;
}

.settings label {
  display: flex;
  flex-direction: column;
  gap: 4px;
}

.settings input {
  width: 100%;
  min-width: 0;
  height: 30px;
  padding: 0 8px;
  border: 1px solid var(--ink-faint);
  border-radius: 8px;
  background: rgba(255, 255, 255, 0.75);
  font: inherit;
}

.settings input[type='checkbox'] {
  flex: 0 0 auto;
  width: 16px;
  height: 16px;
  padding: 0;
}

.settings button {
  align-self: flex-start;
  padding: 6px 14px;
  border: 1px solid var(--ink);
  border-radius: 10px;
  background: #fff;
  font: inherit;
  cursor: pointer;
}

.settings .address-row {
  display: flex;
  align-items: center;
  min-width: 0;
  gap: 8px;
}

.settings .address-row input {
  flex: 1;
  width: auto;
  min-width: 0;
}

.settings .address-row img {
  flex-shrink: 0;
  width: 24px;
  height: 24px;
  object-fit: contain;
}

.settings .hint {
  margin: 0;
  color: var(--ink-soft);
  line-height: 1.45;
  overflow-wrap: anywhere;
}

.settings .hint.pre {
  white-space: pre-wrap;
}

.settings .account {
  display: flex;
  align-items: center;
  flex-wrap: wrap;
  gap: 8px;
  padding: 6px 0;
  border-bottom: 1px dashed var(--ink-faint);
}

.settings .account strong {
  min-width: 0;
  overflow-wrap: anywhere;
}

.settings .account > div {
  display: flex;
  flex: 1;
  flex-direction: column;
}

.settings .check {
  flex-direction: row;
  align-items: center;
}
</style>
