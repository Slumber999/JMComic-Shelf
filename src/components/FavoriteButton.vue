<script setup lang="ts">
import { computed, ref, watch } from 'vue'
import { showError } from '../errors.ts'
import { DropdownOption, NButton, NDropdown, NIcon, useMessage } from 'naive-ui'
import { FavoriteFolderRespData, commands } from '../bindings.ts'
import { useStore } from '../store.ts'
import IconButton from './IconButton.vue'
import { PhStar } from '@phosphor-icons/vue'
import { useI18n } from '../i18n.ts'

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
const { t } = useI18n()
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
const favoriteTitle = computed(() =>
  isFavoriteNow.value ? t('favoriteButton.titleFavorited') : t('favoriteButton.titleFavorite'),
)

const favoriteFolders = ref<FavoriteFolderRespData[]>([])

const favoriteOptions = computed<DropdownOption[]>(() => {
  const options: DropdownOption[] = [
    isFavoriteNow.value
      ? { label: t('favoriteButton.remove'), key: 'remove' }
      : { label: t('favoriteButton.addToAll'), key: 'add' },
  ]

  options.push(
    ...favoriteFolders.value.map((folder) => ({
      label: t('favoriteButton.addToFolder', { name: folder.name }),
      key: `folder:${folder.FID}`,
    })),
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
    message.warning(t('favoriteButton.needLogin'))
    return
  }

  if (key === 'add' || key === 'remove') {
    const result = await commands.toggleFavorite(props.comicId)
    if (result.status === 'error') {
      console.error(result.error)
      showError(result.error)
      return
    }
    isFavoriteNow.value = key === 'add'
    message.success(key === 'add' ? t('favoriteButton.addedToAll') : t('favoriteButton.removed'))
    emit('favoriteChanged')
    return
  }

  if (key.startsWith('folder:')) {
    const folderId = key.slice('folder:'.length)

    if (!isFavoriteNow.value) {
      const favoriteResult = await commands.toggleFavorite(props.comicId)
      if (favoriteResult.status === 'error') {
        console.error(favoriteResult.error)
        showError(favoriteResult.error)
        return
      }
      isFavoriteNow.value = true
    }

    const moveResult = await commands.moveFavoriteToFolder(props.comicId, folderId)
    if (moveResult.status === 'error') {
      console.error(moveResult.error)
      showError(moveResult.error)
      return
    }

    const folder = favoriteFolders.value.find((item) => item.FID === folderId)
    message.success(t('favoriteButton.added', { name: folder?.name ?? folderId }))
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
