import { ref } from 'vue'

/// 移动端历史页的筛选状态：放在模块级，进详情再返回不会丢
export const historyOnlyOngoing = ref(false)
/// 日历筛选的日期（YYYY-MM-DD），空表示不筛
export const historySelectedDate = ref<string>()
