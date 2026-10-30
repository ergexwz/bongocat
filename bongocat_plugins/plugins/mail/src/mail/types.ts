export interface MailAccount {
  id: string
  address: string
  imapHost: string
  imapPort: number
  username: string
  providerDomain: string
  enabled: boolean
  status: 'idle' | 'connecting' | 'connected' | 'error'
  lastSeenUid: number
}

export interface MailNotification {
  id: string
  accountId: string
  uid: number
  from: string
  subject: string
  arrivedAt: number
  status: 'unread' | 'read' | 'archived'
  readAt?: number
  archivedAt?: number
}

export interface MailSettings {
  proxy: string
  bubbleEnabled: boolean
  bubbleAutoDismiss: boolean
  unreadOnly: boolean
}

export interface MailUiSettings {
  listScale: number
  archiveScale: number
  settingsScale: number
}

export interface MailSnapshot {
  accounts: MailAccount[]
  notifications: MailNotification[]
  settings: MailSettings
  ui: MailUiSettings
}
