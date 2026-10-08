import { computed, ref } from 'vue'
import zhCN from './locales/zh-CN.ts'
import zhTW from './locales/zh-TW.ts'

export type Locale = 'zh-CN' | 'zh-TW'
export type Messages = Record<string, unknown>

export const localeOptions: { value: Locale; label: string }[] = [
  { value: 'zh-CN', label: '简体中文' },
  { value: 'zh-TW', label: '繁體中文' },
]

const messages: Record<Locale, Messages> = {
  'zh-CN': zhCN,
  'zh-TW': zhTW,
}

/// 当前语言：组件里读它就会跟着切换重新渲染
export const locale = ref<Locale>('zh-CN')

export function setLocale(next: Locale) {
  if (messages[next] !== undefined) {
    locale.value = next
  }
}

/// 按点号路径取文案，取不到就退回中文、再退回 key 本身（方便发现漏翻）
function lookup(source: Messages, key: string): string | undefined {
  let node: unknown = source
  for (const part of key.split('.')) {
    if (node === null || typeof node !== 'object') {
      return undefined
    }
    node = (node as Record<string, unknown>)[part]
  }
  return typeof node === 'string' ? node : undefined
}

/// 取文案，支持 {name} 占位符
export function t(key: string, params?: Record<string, string | number>): string {
  const raw = lookup(messages[locale.value], key) ?? lookup(messages['zh-CN'], key) ?? key
  if (params === undefined) {
    return raw
  }
  return raw.replace(/\{(\w+)\}/g, (match, name: string) => {
    const value = params[name]
    return value === undefined ? match : String(value)
  })
}

/// 组件里用这个，模板中保持 t('xxx') 的写法
export function useI18n() {
  return { t, locale, setLocale, currentLocale: computed(() => locale.value) }
}
