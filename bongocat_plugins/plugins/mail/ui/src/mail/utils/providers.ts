/**
 * 常见邮箱 IMAP 配置速查 + 授权码获取指引（T1 简化版 provider 识别）。
 *
 * 用户在表单输入邮箱地址后，按域名（`@` 后段）匹配本表自动填充 IMAP host/port，
 * 并展示该邮箱的「授权码获取指引」。T3 会扩展为完整 provider 识别（含 logo 自动识别）。
 *
 * 未命中域名时返回 null，由调用方让用户手动填。
 */
export interface ProviderPreset {
  /** 匹配的域名列表（小写，不含 @）。 */
  domains: string[]
  /** IMAP 服务器地址。 */
  imapHost: string
  /** IMAP 端口（IMAPS 固定 993）。 */
  imapPort: number
  /** 该邮箱获取授权码/专用密码的指引文案（i18n key 后缀，见 locale 文件 providers.*）。 */
  hintKey: string
  /** 该邮箱的展示名（如「Gmail」「QQ邮箱」「Outlook」），气泡来源标签用。 */
  displayName: string
  /** 该邮箱的 webmail 入口 URL（点气泡跳转用）。 */
  webmailUrl: string
  /**
   * 该邮箱的 logo 静态资源路径（vite public 资源，绝对路径 `/mail-logos/...`）。
   *
   * 与项目字体加载一致用 public 目录（原样拷贝、不做 hash、生产/开发同路径）。
   * 全部 RGBA 透明背景，适配暗色模式。
   */
  logo: string
}

/** 内置常见邮箱配置（覆盖国内外主流邮箱）。 */
export const PROVIDER_PRESETS: ProviderPreset[] = [
  {
    domains: ['gmail.com', 'googlemail.com'],
    imapHost: 'imap.gmail.com',
    imapPort: 993,
    hintKey: 'gmail',
    displayName: 'Gmail',
    webmailUrl: 'https://mail.google.com',
    logo: '/mail-logos/logo-gmail.svg',
  },
  {
    domains: ['qq.com'],
    imapHost: 'imap.qq.com',
    imapPort: 993,
    hintKey: 'qq',
    displayName: 'QQ',
    webmailUrl: 'https://mail.qq.com',
    logo: '/mail-logos/logo-qq.png',
  },
  {
    domains: ['foxmail.com'],
    imapHost: 'imap.qq.com',
    imapPort: 993,
    hintKey: 'qq',
    displayName: 'Foxmail',
    webmailUrl: 'https://mail.qq.com',
    logo: '/mail-logos/logo-foxmail-icon.png',
  },
  {
    domains: ['163.com', 'yeah.net'],
    imapHost: 'imap.163.com',
    imapPort: 993,
    hintKey: '163',
    displayName: '163',
    webmailUrl: 'https://mail.163.com',
    logo: '/mail-logos/logo-163-icon.png',
  },
  {
    domains: ['126.com'],
    imapHost: 'imap.126.com',
    imapPort: 993,
    hintKey: '126',
    displayName: '126',
    webmailUrl: 'https://mail.126.com',
    logo: '/mail-logos/logo-126-icon.png',
  },
  {
    domains: ['outlook.com', 'hotmail.com', 'live.com', 'msn.com'],
    imapHost: 'outlook.office365.com',
    imapPort: 993,
    hintKey: 'outlook',
    displayName: 'Outlook',
    webmailUrl: 'https://outlook.live.com',
    logo: '/mail-logos/logo-outlook.svg',
  },
  {
    domains: ['icloud.com', 'me.com', 'mac.com'],
    imapHost: 'imap.mail.me.com',
    imapPort: 993,
    hintKey: 'icloud',
    displayName: 'iCloud',
    webmailUrl: 'https://www.icloud.com/mail',
    logo: '/mail-logos/logo-icloud.svg',
  },
  {
    // ⚠️ Proton 不支持直连 IMAP，须本地运行 Proton Mail Bridge 桥接（默认监听 127.0.0.1:1143）。
    // 此处保留预设只为 logo 自动识别 + 指引文案；连接前请按指引改 host/port 为 Bridge 实际地址。
    domains: ['proton.me', 'protonmail.com'],
    imapHost: '127.0.0.1',
    imapPort: 1143,
    hintKey: 'proton',
    displayName: 'Proton',
    webmailUrl: 'https://mail.proton.me',
    logo: '/mail-logos/logo-proton.png',
  },
  {
    domains: ['yahoo.com'],
    imapHost: 'imap.mail.yahoo.com',
    imapPort: 993,
    hintKey: 'yahoo',
    displayName: 'Yahoo',
    webmailUrl: 'https://mail.yahoo.com',
    logo: '/mail-logos/logo-yahoo.svg',
  },
]

/**
 * 按邮箱地址匹配 provider 预设。
 *
 * 匹配顺序：①精确域名命中预设 → ②后缀模式（教育邮箱 `.edu.cn` / `.edu`）。
 * 教育邮箱是 SaaS（icoremail），域名千变万化，无法逐一枚举，按后缀兜底。
 *
 * @returns 命中的 ProviderPreset；未命中返回 null（调用方让用户手动填）
 */
export function matchProvider(address: string): ProviderPreset | null {
  const at = address.lastIndexOf('@')
  if (at < 0) {
    return null
  }
  const domain = address.slice(at + 1).toLowerCase()

  // ① 精确域名匹配
  const exact = PROVIDER_PRESETS.find(p => p.domains.includes(domain))
  if (exact) {
    return exact
  }

  // ② 后缀模式匹配（教育邮箱等无法枚举的域名）
  for (const pattern of SUFFIX_PATTERNS) {
    if (domain.endsWith(pattern.suffix)) {
      return {
        domains: [domain],
        imapHost: pattern.imapHost,
        imapPort: pattern.imapPort,
        hintKey: pattern.hintKey,
        displayName: pattern.extractDisplayName?.(address, domain) ?? '教育邮箱',
        webmailUrl: pattern.webmailUrlTemplate,
        logo: EDU_MAIL_LOGO,
      }
    }
  }

  return null
}

/** 未识别邮箱的默认 logo（中性灰信封）。 */
export const DEFAULT_MAIL_LOGO = '/mail-logos/logo-mail-default.svg'

/** 教育邮箱通用 logo（毕业帽，学院蓝）。 */
const EDU_MAIL_LOGO = '/mail-logos/logo-edu.svg'

/**
 * 后缀模式匹配（精确域名未命中时的兜底）。
 *
 * 教育邮箱（`.edu.cn` / `.edu`）是 Coremail/iCoremail SaaS，域名千变万化
 * （如 `s.ytu.edu.cn`），无法逐一枚举。这里按后缀模式匹配，统一走
 * `edu.icoremail.net` IMAP + `edu.icoremail.net` webmail。
 * displayName 从域名提取学校简称（如 `s.ytu.edu.cn` → YTU）。
 */
interface SuffixPattern {
  /** 域名后缀（小写，如 `.edu.cn`）。匹配时 endsWith。 */
  suffix: string
  /** IMAP 服务器。 */
  imapHost: string
  /** IMAP 端口。 */
  imapPort: number
  /** 授权码指引 i18n key 后缀。 */
  hintKey: string
  /** webmail URL 模板（`{address}` 替换为完整邮箱地址）。 */
  webmailUrlTemplate: string
  /**
   * 从完整邮箱地址提取展示名。返回 null 时用 displayName 兜底。
   * 教育邮箱提取学校简称（如 `s.ytu.edu.cn` → YTU）。
   */
  extractDisplayName?: (address: string, domain: string) => string | null
}

const SUFFIX_PATTERNS: SuffixPattern[] = [
  {
    suffix: '.edu.cn',
    imapHost: 'edu.icoremail.net',
    imapPort: 993,
    hintKey: 'edu',
    webmailUrlTemplate: 'https://edu.icoremail.net',
    extractDisplayName: (_address, domain) => extractEduAbbr(domain),
  },
  {
    suffix: '.edu',
    imapHost: 'edu.icoremail.net',
    imapPort: 993,
    hintKey: 'edu',
    webmailUrlTemplate: 'https://edu.icoremail.net',
    extractDisplayName: (_address, domain) => extractEduAbbr(domain),
  },
]

/**
 * 从教育邮箱域名提取学校简称。
 *
 * 教育域名格式通常为 `<子域>.<学校简称>.edu.cn`（如 `s.ytu.edu.cn` → YTU，
 * `mail.tsinghua.edu.cn` → TSINGHUA）。取 `.edu.cn` 前最后一段作为简称，大写。
 * 无法提取时返回 null（用 displayName "教育邮箱" 兜底）。
 */
function extractEduAbbr(domain: string): string | null {
  // 去掉 .edu.cn / .edu 后缀
  const stripped = domain.replace(/\.edu(\.cn)?$/, '')
  // 按 . 分割，取倒数第一段（学校简称段）
  const parts = stripped.split('.')
  const last = parts[parts.length - 1]
  if (!last || last.length < 2) {
    return null
  }
  return last.toUpperCase()
}

/**
 * 取邮箱地址对应的 logo 资源路径。
 *
 * 命中预设返回 `preset.logo`；未命中（自定义域名邮箱）返回默认信封 logo。
 * 设置页账号列表 + 添加表单输入框左侧 logo 联动都用本函数。
 */
export function matchProviderLogo(address: string): string {
  return matchProvider(address)?.logo ?? DEFAULT_MAIL_LOGO
}

/**
 * 解析邮箱的 webmail 入口 URL。
 *
 * 点击邮件气泡跳转用。按邮箱地址匹配 provider（含后缀模式）取 webmailUrl；
 * 未命中时回退按 IMAP host 匹配预设。都未命中返回 null。
 *
 * @param address 邮箱地址（首选匹配键）
 * @param imapHost IMAP 服务器地址（address 未命中时的回退匹配键）
 */
export function resolveWebmailUrl(address: string, imapHost?: string): string | null {
  const byAddr = matchProvider(address)
  if (byAddr) {
    return byAddr.webmailUrl
  }
  if (imapHost) {
    const host = imapHost.toLowerCase()
    // 先查精确预设
    const byHost = PROVIDER_PRESETS.find(p => p.imapHost === host)
    if (byHost) {
      return byHost.webmailUrl
    }
    // 再查后缀模式（教育邮箱 IMAP host = edu.icoremail.net）
    const bySuffix = SUFFIX_PATTERNS.find(p => p.imapHost === host)
    if (bySuffix) {
      return bySuffix.webmailUrlTemplate
    }
  }
  return null
}
