import type { Component } from 'vue'
import {
  PhBookmarkSimple,
  PhChatCircleDots,
  PhGearSix,
  PhHardDrives,
  PhMagnifyingGlass,
} from '@phosphor-icons/vue'
import SearchScreen from './screens/SearchScreen.vue'
import FavoriteScreen from './screens/FavoriteScreen.vue'
import LocalScreen from './screens/LocalScreen.vue'
import CommentsScreen from './screens/CommentsScreen.vue'
import SettingsScreen from './screens/SettingsScreen.vue'
import type { MobileScreen } from './useNavStack.ts'

/// 外壳 5 屏的唯一顺序来源：滑动切页、扇形菜单布局、高亮、切换全部由它派生
export const SHELL_SCREENS = ['search', 'favorite', 'local', 'comments', 'settings'] as const

export type ShellScreen = (typeof SHELL_SCREENS)[number]

export const SHELL_SCREEN_COMPONENTS: Record<ShellScreen, Component> = {
  search: SearchScreen,
  favorite: FavoriteScreen,
  local: LocalScreen,
  comments: CommentsScreen,
  settings: SettingsScreen,
}

/// 扇形菜单用的标题 / 图标：顺序同样由 SHELL_SCREENS 决定
export const SHELL_SCREEN_META: Record<ShellScreen, { labelKey: string; icon: Component }> = {
  search: { labelKey: 'tab.search', icon: PhMagnifyingGlass },
  favorite: { labelKey: 'tab.favorite', icon: PhBookmarkSimple },
  local: { labelKey: 'tab.downloaded', icon: PhHardDrives },
  comments: { labelKey: 'tab.comments', icon: PhChatCircleDots },
  settings: { labelKey: 'mobile.menuSettings', icon: PhGearSix },
}

export function isShellScreen(screen: MobileScreen): screen is ShellScreen {
  return (SHELL_SCREENS as readonly string[]).includes(screen)
}

export function shellScreenIndex(screen: MobileScreen): number {
  return (SHELL_SCREENS as readonly string[]).indexOf(screen)
}
