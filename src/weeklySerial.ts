/// 每周连载更新的两组筛选项：类型（全部/日漫/韩漫）+ 星期（周一…周日）。
/// 桌面「每周连载」页签和手机搜索页共用这一份，文案走语言包。
export const WEEKLY_TYPES = [
  { value: 'all', labelKey: 'ranking.all' },
  { value: 'manga', labelKey: 'mobile.weeklyManga' },
  { value: 'hanman', labelKey: 'mobile.weeklyHanman' },
] as const

/// 接口的 date 从 1 开始：1 = 周一 … 7 = 周日
export const WEEKLY_DAYS = [
  { date: 1, labelKey: 'weekly.monday' },
  { date: 2, labelKey: 'weekly.tuesday' },
  { date: 3, labelKey: 'weekly.wednesday' },
  { date: 4, labelKey: 'weekly.thursday' },
  { date: 5, labelKey: 'weekly.friday' },
  { date: 6, labelKey: 'weekly.saturday' },
  { date: 7, labelKey: 'weekly.sunday' },
] as const

/// 今天对应的接口 date：周日给 7（JS 的 getDay() 里周日是 0）
export function todayWeekday(): number {
  const day = new Date().getDay()
  return day === 0 ? 7 : day
}
