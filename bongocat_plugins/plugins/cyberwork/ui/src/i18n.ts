import { createI18n } from 'vue-i18n'

const zhCN = {
  cyberwork: {
    title: '使用历史',
    taps: '敲击次数',
    game: '游戏时长',
    total: '累计',
    today: '最近一天',
    week: '最近一周',
    month: '最近一个月',
    from: '起点 {date}',
    close: '关闭',
    empty: '还没有记录，动动键盘或手柄吧～',
    count: '{count} 次',
    error: {
      data_could_not_be_read: '历史数据读取失败，已保留原文件并从空记录开始。',
      data_schema_unsupported: '历史数据版本不受支持，已备份原文件并从空记录开始。',
      data_backup_failed: '历史数据无法备份，已暂停写入以保护原文件。',
      data_could_not_be_saved: '历史数据保存失败，计数仍在内存中继续。',
      unknown: '历史数据暂时不可用。',
    },
    duration: {
      day: '天',
      hour: '小时',
      minute: '分',
      second: '秒',
      zero: '0 秒',
    },
  },
}

const enUS = {
  cyberwork: {
    title: 'Usage History',
    taps: 'Taps',
    game: 'Game time',
    total: 'Total',
    today: 'Last day',
    week: 'Last week',
    month: 'Last month',
    from: 'From {date}',
    close: 'Close',
    empty: 'No records yet. Use the keyboard or a gamepad.',
    count: '{count} taps',
    error: {
      data_could_not_be_read: 'History data could not be read. The original file was kept and counting restarted empty.',
      data_schema_unsupported: 'History data has an unsupported version. It was backed up and counting restarted empty.',
      data_backup_failed: 'History data could not be backed up, so writing is paused to protect the original file.',
      data_could_not_be_saved: 'History data could not be saved; counting continues in memory.',
      unknown: 'History data is temporarily unavailable.',
    },
    duration: {
      day: 'd',
      hour: 'h',
      minute: 'm',
      second: 's',
      zero: '0s',
    },
  },
}

export const i18n = createI18n({
  legacy: false,
  locale: 'en-US',
  fallbackLocale: 'en-US',
  messages: { 'zh-CN': zhCN, 'en-US': enUS },
})
