<script setup lang="ts">
import { computed, h } from 'vue'
import { DropdownOption, NButton, NDropdown, NIcon } from 'naive-ui'
import { PhCheck } from '@phosphor-icons/vue'
import { splitAuthorNames } from '../authorNames.ts'
import { useFavoriteAuthors } from '../favoriteAuthors.ts'
import { useI18n } from '../i18n.ts'

const props = defineProps<{ author: string | string[] }>()

const { t } = useI18n()
const { isFavoriteAuthor, toggleFavoriteAuthor } = useFavoriteAuthors()

/// 作者字段可能是一个带分隔符的字符串，也可能是数组：统一拆成单个作者名；作者为空就没有按钮
const authors = computed(() => splitAuthorNames(props.author))
const favoritedCount = computed(() => authors.value.filter((name) => isFavoriteAuthor(name)).length)

/// 多作者：点按钮弹出选择，列表里已收藏的打勾，点一下即切换（选完自动关闭）
const authorOptions = computed<DropdownOption[]>(() =>
  authors.value.map((name) => ({
    key: name,
    label: name,
    icon: isFavoriteAuthor(name)
      ? () => h(NIcon, { size: 16 }, { default: () => h(PhCheck) })
      : undefined,
  })),
)

const buttonText = computed(() => {
  if (authors.value.length <= 1) {
    return isFavoriteAuthor(authors.value[0] ?? '') ? t('author.favorited') : t('author.favorite')
  }
  return favoritedCount.value === 0
    ? t('author.favorite')
    : t('author.favoritedMulti', { favorited: favoritedCount.value, total: authors.value.length })
})

function onAuthorSelect(key: string | number): void {
  toggleFavoriteAuthor(String(key))
}
</script>

<template>
  <!-- 文字按钮（text 按钮没有边框和固定高度），不会把作者那一行撑高 -->
  <span v-if="authors.length > 0" class="inline-flex shrink-0 items-center gap-2">
    <!-- 只有一个作者：直接切换，不弹选择 -->
    <n-button
      v-if="authors.length === 1"
      text
      size="tiny"
      :type="isFavoriteAuthor(authors[0]) ? 'primary' : 'default'"
      :title="t('author.favoriteHint', { name: authors[0] })"
      @click="toggleFavoriteAuthor(authors[0])">
      {{ buttonText }}
    </n-button>
    <!-- 多个作者：弹出选择要收藏哪个 -->
    <n-dropdown v-else trigger="click" :options="authorOptions" @select="onAuthorSelect">
      <n-button text size="tiny" :type="favoritedCount > 0 ? 'primary' : 'default'" :title="t('author.favoriteMultiHint')">
        {{ buttonText }}
      </n-button>
    </n-dropdown>
  </span>
</template>
