import { computed, ref } from 'vue'
import type { NotificationItem } from '../bindings.ts'
import { commands } from '../bindings.ts'
import { showError } from '../errors.ts'
import { useStore } from '../store.ts'

/// 通知状态是模块级单例：配置页关掉后工具栏红点还得留着，组件卸载不能丢。
const notification = ref<NotificationItem>()
const markingRead = ref<boolean>(false)
/// 红点跟着官方 read 字段走：最新一条未读就亮
const hasUnreadNotification = computed(() => notification.value?.read === false)

/// 列表顺序不保证，按 id 取最大的一条：界面只展示最新这一条
function pickLatestNotification(list: NotificationItem[]): NotificationItem | undefined {
  let latest = list[0]
  for (const item of list) {
    if (item.id > latest.id) {
      latest = item
    }
  }
  return latest
}

/// 拉最新一条通知：未登录直接清空，不发请求
async function refreshNotifications() {
  const store = useStore()
  if (store.userProfile === undefined) {
    notification.value = undefined
    return
  }

  const result = await commands.getNotifications()
  if (result.status === 'error') {
    console.error(result.error)
    return
  }
  notification.value = pickLatestNotification(result.data.list ?? [])
}

/// 标记最新一条为已读：成功后全局 read 置 true，工具栏红点立即消失
async function markNotificationRead(): Promise<boolean> {
  const current = notification.value
  if (current === undefined || current.read !== false || markingRead.value) {
    return false
  }

  markingRead.value = true
  const result = await commands.markNotificationRead(current.id)
  markingRead.value = false
  if (result.status === 'error') {
    showError(result.error)
    return false
  }

  notification.value = { ...current, read: true }
  return true
}

/// 组件只读这里的 ref：状态由 AppContent 在登录/打开配置页时刷新
export function useNotifications() {
  return { notification, markingRead, hasUnreadNotification, refreshNotifications, markNotificationRead }
}
