<script setup lang="ts">
import { computed, ref, watch } from 'vue'
import { DropdownOption, NButton, NDropdown, NIcon, useMessage } from 'naive-ui'
import { FavoriteFolderRespData, commands } from '../bindings.ts'
import { useStore } from '../store.ts'
import IconButton from './IconButton.vue'
import { PhStar } from '@phosphor-icons/vue'

/// variant: 'icon' 用于漫画卡片，'button' 用于阅读器工具栏
const props = withDefaults(
  defineProps<{
    comicId: number
    isFavorite?: boolean
    variant?: 'icon' | 'button'
  }>(),
  { isFavorite: false, variant: 'icon' },
)

const emit = defineEmits<{ favoriteChanged: [] }>()

const store = useStore()
const message = useMessage()

const isFavoriteNow = ref<boolean>(false)
watch(
  () => props.isFavorite,
  (value) => {
    isFavoriteNow.value = value
  },
  { immediate: true },
)

const starWeight = computed<'fill' | 'regular'>(() => (isFavoriteNow.value ? 'fill' : 'regular'))
const favoriteTitle = computed(() => (isFavoriteNow.value ? '已收藏（可移动或取消）' : '收藏'))

const favoriteFolders = ref<FavoriteFolderRespData[]>([])

const favoriteOptions = computed<DropdownOption[]>(() => {
  const options: DropdownOption[] = [
    isFavoriteNow.value ? { label: '取消收藏', key: 'remove' } : { label: '收藏到全部', key: 'add' },
  ]

  options.push(
    ...favoriteFolders.value.map((folder) => ({ label: `收藏到「${folder.name}」`, key: `folder:${folder.FID}` })),
  )

  return options
})

async function loadFavoriteFolders() {
  if (store.userProfile === undefined) {
    return
  }

  const result = await commands.getFavoriteFolders()
  if (result.status === 'error') {
    console.error(result.error)
    return
  }
  favoriteFolders.value = result.data
}

function onFavoriteDropdownShow(show: boolean) {
  if (show) {
    void loadFavoriteFolders()
  }
}

async function onFavoriteSelect(key: string) {
  if (store.userProfile === undefined) {
    message.warning('请先登录')
    return
  }

  if (key === 'add' || key === 'remove') {
    const result = await commands.toggleFavorite(props.comicId)
    if (result.status === 'error') {
      console.error(result.error)
      message.error(result.error.message, { duration: 8000 })
      return
    }
    isFavoriteNow.value = key === 'add'
    message.success(key === 'add' ? '已收藏到「全部」' : '已取消收藏')
    emit('favoriteChanged')
    return
  }

  if (key.startsWith('folder:')) {
    const folderId = key.slice('folder:'.length)

    if (!isFavoriteNow.value) {
      const favoriteResult = await commands.toggleFavorite(props.comicId)
      if (favoriteResult.status === 'error') {
        console.error(favoriteResult.error)
        message.error(favoriteResult.error.message, { duration: 8000 })
        return
      }
      isFavoriteNow.value = true
    }

    const moveResult = await commands.moveFavoriteToFolder(props.comicId, folderId)
    if (moveResult.status === 'error') {
      console.error(moveResult.error)
      message.error(moveResult.error.message, { duration: 8000 })
      return
    }

    const folder = favoriteFolders.value.find((item) => item.FID === folderId)
    message.success(`已收藏到「${folder?.name ?? folderId}」`)
    emit('favoriteChanged')
  }
}
</script>

<template>
  <n-dropdown
    trigger="click"
    :options="favoriteOptions"
    :show-arrow="false"
    @select="onFavoriteSelect"
    @update:show="onFavoriteDropdownShow">
    <span>
      <IconButton v-if="variant === 'icon'" :title="favoriteTitle">
        <PhStar :size="20" :weight="starWeight" :class="isFavoriteNow ? 'text-yellow-5' : ''" />
      </IconButton>
      <n-button v-else size="small" :type="isFavoriteNow ? 'primary' : 'default'" :title="favoriteTitle">
        <template #icon>
          <n-icon size="18">
            <PhStar :weight="starWeight" />
          </n-icon>
        </template>
      </n-button>
    </span>
  </n-dropdown>
</template>
