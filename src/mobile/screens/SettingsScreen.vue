<script setup lang="ts">
import { computed, onMounted, ref } from 'vue'
import {
  NButton,
  NCheckbox,
  NIcon,
  NPopconfirm,
  NRadio,
  NRadioButton,
  NRadioGroup,
  useMessage,
} from 'naive-ui'
import { PhTrash } from '@phosphor-icons/vue'
import { getVersion } from '@tauri-apps/api/app'
import AccountSettings from '../../dialogs/SettingsDialog/components/AccountSettings.vue'
import {
  commands,
  type FavoritesFolderIndex,
  type Language,
  type StorageEntry,
  type StorageStats,
} from '../../bindings.ts'
import { showError } from '../../errors.ts'
import { formatBytes } from '../../formatBytes.ts'
import { localeOptions, setLocale, useI18n, type Locale } from '../../i18n.ts'
import { useStore } from '../../store.ts'
import { readerHand, setReaderHand } from '../useReaderHand.ts'

const store = useStore()
const { t } = useI18n()
const message = useMessage()

/// 手机端设置一页四段：账号 → 界面 → 空间 → 关于，不再做横向分组
const REPO_URL = 'https://github.com/Slumber999/JMComic-Shelf'
const version = ref('')

/// 空间段与桌面 StorageSettings 同款，只去掉「刷新」和「打开文件夹」两个按钮
const stats = ref<StorageStats>()
const loading = ref(true)
/// 正在清理哪一项（用来显示 loading，同时避免重复点击）
const cleaning = ref<string>()
const selectedQuickReaders = ref<string[]>([])

/// 收藏索引：只看/重建 folderId=0 这层（「全部收藏」），其余层在收藏夹页按需建立
const favoritesIndex = ref<FavoritesFolderIndex | null>(null)
const rebuildingIndex = ref(false)

const indexStatusText = computed(() => {
  if (favoritesIndex.value === null) {
    return t('settings.storage.indexMissing')
  }
  return t('settings.storage.indexStatus', {
    time: new Date(favoritesIndex.value.updatedAt).toLocaleString(),
    count: favoritesIndex.value.total,
  })
})

const leftoverBytes = computed(() =>
  (stats.value?.leftovers ?? []).reduce((sum, item) => sum + item.bytes, 0),
)
const quickReaderBytes = computed(() =>
  (stats.value?.quickReaders ?? []).reduce((sum, item) => sum + item.bytes, 0),
)

async function refreshFavoritesIndex() {
  try {
    favoritesIndex.value = await commands.getFavoritesIndex(0)
  } catch (error) {
    console.error(error)
  }
}

/// 全量重建：整层覆盖，建完刷新状态并把总数报给用户
async function rebuildFavoritesIndex() {
  rebuildingIndex.value = true
  try {
    const result = await commands.buildFavoritesIndex(0)
    if (result.status === 'error') {
      showError(result.error)
      return
    }
    await refreshFavoritesIndex()
    message.success(t('settings.storage.indexRebuilt', { total: result.data.total }))
  } catch (error) {
    console.error(error)
  } finally {
    rebuildingIndex.value = false
  }
}

type CleanResult = Awaited<ReturnType<typeof commands.cleanStorageLogs>>

async function refresh() {
  loading.value = true
  try {
    const result = await commands.getStorageStats()
    if (result.status === 'error') {
      showError(result.error)
      return
    }
    stats.value = result.data
    // 去掉已经不存在的勾选
    const paths = new Set(result.data.quickReaders.map((item) => item.path))
    selectedQuickReaders.value = selectedQuickReaders.value.filter((path) => paths.has(path))
  } catch (error) {
    console.error(error)
  } finally {
    loading.value = false
  }
}

function handleCleanResult(result: CleanResult, what: string) {
  if (result.status === 'error') {
    showError(result.error)
    return
  }
  if (result.data === 0) {
    message.info(t('settings.storage.nothingToClean', { what }))
  } else {
    message.success(t('settings.storage.cleaned', { what, size: formatBytes(result.data) }))
  }
  void refresh()
}

/// 清理类操作共用一套：标记正在清理哪一项（按钮转圈、顺便防连点），结果交给 handleCleanResult
async function runClean(key: string, clean: () => Promise<CleanResult>, what: string) {
  cleaning.value = key
  try {
    handleCleanResult(await clean(), what)
  } finally {
    cleaning.value = undefined
  }
}

function cleanLeftovers() {
  void runClean('leftovers', () => commands.cleanStorageLeftovers(), t('settings.storage.leftovers'))
}

function cleanQuickReaders() {
  if (selectedQuickReaders.value.length === 0) {
    message.warning(t('settings.storage.pickQuickReadersFirst'))
    return
  }
  void runClean(
    'quickReaders',
    () => commands.cleanStorageQuickReaders(selectedQuickReaders.value),
    t('settings.storage.quickReaders'),
  )
}

function cleanLogs() {
  void runClean('logs', () => commands.cleanStorageLogs(), t('settings.storage.oldLogs'))
}

async function deleteComic(entry: StorageEntry) {
  const comicId = entry.comicId
  const source = entry.source
  if (comicId === null || source === null) {
    return
  }
  await runClean(
    `comic-${comicId}-${source}`,
    () => commands.deleteLocalComic(comicId, source),
    t('settings.storage.comics'),
  )
}

function toggleQuickReader(path: string) {
  selectedQuickReaders.value = selectedQuickReaders.value.includes(path)
    ? selectedQuickReaders.value.filter((item) => item !== path)
    : [...selectedQuickReaders.value, path]
}

onMounted(async () => {
  try {
    version.value = await getVersion()
  } catch {
    // 浏览器预览没有 Tauri 后端，保留空版本号
    version.value = ''
  }

  void refresh()
  void refreshFavoritesIndex()
})

function onLanguageChange(value: string | number) {
  if (store.config === undefined) {
    return
  }
  store.config.language = value as Language
  setLocale(value as Locale)
}

function onReaderHandChange(value: string) {
  if (value === 'left' || value === 'right') {
    setReaderHand(value)
  }
}

/// 仓库地址整行可点，点了复制链接
async function copyRepoUrl() {
  if (navigator.clipboard?.writeText === undefined) {
    message.warning(t('common.copyFailed'))
    return
  }
  try {
    await navigator.clipboard.writeText(REPO_URL)
    message.success(t('common.copied'))
  } catch (error) {
    console.error(error)
    message.warning(t('common.copyFailed'))
  }
}
</script>

<template>
  <section class="settings">
    <section class="block">
      <h2 class="block-title">{{ t('settings.tab.account') }}</h2>
      <account-settings />
    </section>

    <section class="block">
      <h2 class="block-title">{{ t('settings.tab.interface') }}</h2>
      <div class="field">
        <span class="field-label">{{ t('settings.language') }}</span>
        <n-radio-group :value="store.config?.language" size="small" @update:value="onLanguageChange">
          <n-radio v-for="option in localeOptions" :key="option.value" :value="option.value">
            {{ option.label }}
          </n-radio>
        </n-radio-group>
        <span class="field-hint">{{ t('settings.languageHint') }}</span>
      </div>
      <div class="field">
        <span class="field-label">{{ t('mobile.readerHandLabel') }}</span>
        <n-radio-group :value="readerHand" size="small" @update:value="onReaderHandChange">
          <n-radio-button value="left">{{ t('mobile.readerHandLeft') }}</n-radio-button>
          <n-radio-button value="right">{{ t('mobile.readerHandRight') }}</n-radio-button>
        </n-radio-group>
        <span class="field-hint">{{ t('mobile.readerHandHint') }}</span>
      </div>
    </section>

    <section class="block">
      <h2 class="block-title">{{ t('settings.tab.storage') }}</h2>

      <!-- 占用统计：三个目录的字节数、数量与路径，刷新按钮已去掉 -->
      <div class="sub-head">
        <span class="field-label">{{ t('settings.storage.usage') }}</span>
        <span v-if="loading" class="field-hint">{{ t('settings.storage.counting') }}</span>
      </div>
      <div v-if="stats !== undefined" class="dir-list">
        <div class="dir-row">
          <div class="dir-line">
            <span class="dir-name">{{ t('settings.download.dir') }}</span>
            <span class="usage-size">{{ formatBytes(stats.download.bytes) }}</span>
            <span class="usage-count">{{ t('settings.storage.comicCount', { count: stats.download.count }) }}</span>
          </div>
          <span class="dir-path" :title="stats.download.path">{{ stats.download.path }}</span>
        </div>
        <div class="dir-row">
          <div class="dir-line">
            <span class="dir-name">{{ t('settings.export.dir') }}</span>
            <span class="usage-size">{{ formatBytes(stats.export.bytes) }}</span>
            <span class="usage-count">{{ t('settings.storage.comicCount', { count: stats.export.count }) }}</span>
          </div>
          <span class="dir-path" :title="stats.export.path">{{ stats.export.path }}</span>
        </div>
        <div class="dir-row">
          <div class="dir-line">
            <span class="dir-name">{{ t('settings.storage.logsLabel') }}</span>
            <span class="usage-size">{{ formatBytes(stats.logs.bytes) }}</span>
            <span class="usage-count">{{ t('settings.storage.fileCount', { count: stats.logs.count }) }}</span>
          </div>
          <span class="dir-path" :title="stats.logs.path">{{ stats.logs.path }}</span>
        </div>
      </div>
      <p v-else-if="!loading" class="field-hint">{{ t('settings.storage.countingShort') }}</p>

      <!-- 可清理项：按钮与确认流程保持桌面同款 -->
      <div v-if="stats !== undefined" class="card">
        <span class="card-title">{{ t('settings.storage.cleanable') }}</span>

        <div class="entry-row">
          <span class="entry-text">
            {{ t('settings.storage.leftoversDetail', { count: stats.leftovers.length, size: formatBytes(leftoverBytes) }) }}
          </span>
          <n-popconfirm v-if="stats.leftovers.length > 0" @positive-click="cleanLeftovers">
            <template #trigger>
              <n-button size="small" type="warning" secondary :loading="cleaning === 'leftovers'">
                {{ t('settings.storage.clean') }}
              </n-button>
            </template>
            {{ t('settings.storage.leftoversConfirm', { count: stats.leftovers.length }) }}
          </n-popconfirm>
          <span v-else class="field-hint">{{ t('settings.storage.none') }}</span>
        </div>

        <div class="entry-row">
          <span class="entry-text">
            {{ t('settings.storage.quickReadersDetail', { count: stats.quickReaders.length, size: formatBytes(quickReaderBytes) }) }}
          </span>
          <n-popconfirm v-if="stats.quickReaders.length > 0" @positive-click="cleanQuickReaders">
            <template #trigger>
              <n-button
                size="small"
                type="warning"
                secondary
                :disabled="selectedQuickReaders.length === 0"
                :loading="cleaning === 'quickReaders'">
                {{ t('settings.storage.cleanSelected', { count: selectedQuickReaders.length }) }}
              </n-button>
            </template>
            {{ t('settings.storage.quickReadersNote') }}
          </n-popconfirm>
          <span v-else class="field-hint">{{ t('settings.storage.none') }}</span>
        </div>
        <div v-if="stats.quickReaders.length > 0" class="checkbox-list">
          <n-checkbox
            v-for="item in stats.quickReaders"
            :key="item.path"
            :checked="selectedQuickReaders.includes(item.path)"
            @update:checked="toggleQuickReader(item.path)">
            {{ item.name }} · {{ formatBytes(item.bytes) }}
          </n-checkbox>
        </div>

        <div class="entry-row">
          <span class="entry-text">
            {{ t('settings.storage.logsDetail', { count: stats.logs.count, size: formatBytes(stats.logs.bytes) }) }}
          </span>
          <n-popconfirm @positive-click="cleanLogs">
            <template #trigger>
              <n-button size="small" type="warning" secondary :loading="cleaning === 'logs'">
                {{ t('settings.storage.cleanOldLogs') }}
              </n-button>
            </template>
            {{ t('settings.storage.logsNote') }}
          </n-popconfirm>
        </div>
      </div>

      <!-- 占用最大的漫画：只保留删除，打开文件夹按钮已去掉 -->
      <div v-if="stats !== undefined" class="card">
        <span class="card-title">{{ t('settings.storage.biggestComics', { count: stats.biggestComics.length }) }}</span>
        <span v-if="stats.biggestComics.length === 0" class="field-hint">
          {{ t('settings.storage.noComics') }}
        </span>
        <div v-for="item in stats.biggestComics" :key="item.path" class="comic-row">
          <span class="comic-name" :title="item.name">
            {{ item.name }}
            <span v-if="item.source === 'ExportDir'" class="comic-tag">{{ t('settings.storage.exportTag') }}</span>
          </span>
          <span class="comic-size">{{ formatBytes(item.bytes) }}</span>
          <n-popconfirm @positive-click="deleteComic(item)">
            <template #trigger>
              <n-button
                size="tiny"
                quaternary
                type="error"
                :loading="cleaning === `comic-${item.comicId}-${item.source}`">
                <template #icon>
                  <n-icon><PhTrash /></n-icon>
                </template>
              </n-button>
            </template>
            {{
              t('settings.storage.deleteConfirm', {
                name: item.name,
                kind: item.source === 'ExportDir' ? t('settings.storage.exportFile') : t('settings.storage.downloadFile'),
                size: formatBytes(item.bytes),
              })
            }}<br />
            {{ t('settings.storage.filesDeleted') }}
          </n-popconfirm>
        </div>
      </div>

      <!-- 收藏索引：搜索收藏夹要用的本地文件，重建就是整层覆盖 -->
      <div class="card">
        <span class="card-title">{{ t('settings.storage.indexTitle') }}</span>
        <div class="entry-row">
          <span class="entry-text">{{ indexStatusText }}</span>
          <n-button
            class="shrink-0"
            size="small"
            type="primary"
            secondary
            :loading="rebuildingIndex"
            @click="rebuildFavoritesIndex">
            {{ t('settings.storage.indexRebuild') }}
          </n-button>
        </div>
        <span class="field-hint">{{ t('settings.storage.indexNote') }}</span>
      </div>
    </section>

    <section class="block">
      <h2 class="block-title">{{ t('app.about') }}</h2>
      <div class="about-row">
        <span class="about-label">{{ t('about.version') }}</span>
        <span class="about-value">{{ version === '' ? '—' : `v${version}` }}</span>
      </div>
      <button class="repo-row" type="button" :title="t('common.clickToCopy')" @click="copyRepoUrl">
        <span class="about-label">{{ t('mobile.repoAddress') }}</span>
        <span class="repo-value">{{ REPO_URL }}</span>
      </button>
    </section>
  </section>
</template>

<style scoped>
.settings {
  display: flex;
  flex-direction: column;
  gap: 14px;
  min-height: 100%;
  padding: 12px 12px 24px;
  background: #f5f5f6;
}

/* 段与段之间靠留白 + 各自的白底卡片区分，不再用 tabs */
.block {
  padding: 14px;
  border-radius: 14px;
  background: #fff;
}

.block-title {
  margin: 0 0 12px;
  color: #888;
  font-size: 13px;
  font-weight: 600;
}

.field {
  display: flex;
  flex-direction: column;
  gap: 8px;
}

.field + .field {
  margin-top: 16px;
  padding-top: 16px;
  border-top: 1px solid #eee;
}

.field-label {
  color: #333;
  font-size: 13px;
  font-weight: 600;
}

.field-hint {
  margin: 0;
  color: #999;
  font-size: 11px;
  line-height: 1.5;
}

.sub-head {
  display: flex;
  align-items: center;
  gap: 8px;
}

.dir-list {
  display: flex;
  flex-direction: column;
  gap: 12px;
  margin-top: 10px;
}

.dir-row {
  display: flex;
  flex-direction: column;
  gap: 2px;
  min-width: 0;
}

.dir-line {
  display: flex;
  align-items: center;
  gap: 8px;
}

.dir-name {
  flex: 1;
  color: #333;
  font-size: 13px;
}

.usage-size {
  color: #111;
  font-size: 13px;
  font-weight: 600;
}

.usage-count {
  min-width: 54px;
  text-align: right;
  color: #999;
  font-size: 12px;
}

.dir-path {
  overflow: hidden;
  color: #bbb;
  font-size: 11px;
  text-overflow: ellipsis;
  white-space: nowrap;
}

/* 可清理 / 最大占用 / 索引三块：和桌面一样用细边框分区 */
.card {
  display: flex;
  flex-direction: column;
  gap: 10px;
  margin-top: 14px;
  padding: 10px;
  border: 1px solid #eee;
  border-radius: 10px;
}

.card-title {
  color: #333;
  font-size: 13px;
  font-weight: 600;
}

.entry-row {
  display: flex;
  align-items: center;
  gap: 8px;
  font-size: 13px;
}

.entry-text {
  flex: 1;
  min-width: 0;
  color: #333;
  line-height: 1.5;
}

.checkbox-list {
  display: flex;
  flex-direction: column;
  gap: 4px;
  max-height: 128px;
  padding-left: 4px;
  overflow-y: auto;
}

.comic-row {
  display: flex;
  align-items: center;
  gap: 8px;
  font-size: 13px;
}

.comic-name {
  flex: 1;
  min-width: 0;
  overflow: hidden;
  color: #333;
  text-overflow: ellipsis;
  white-space: nowrap;
}

.comic-tag {
  color: #999;
  font-size: 11px;
}

.comic-size {
  flex-shrink: 0;
  color: #333;
}

.about-row {
  display: flex;
  align-items: center;
  justify-content: space-between;
  gap: 8px;
}

.about-label {
  color: #333;
  font-size: 13px;
  font-weight: 600;
}

.about-value {
  color: #555;
  font-size: 13px;
}

.repo-row {
  display: flex;
  flex-direction: column;
  gap: 4px;
  width: 100%;
  margin-top: 14px;
  padding: 14px 0 0;
  border: none;
  border-top: 1px solid #eee;
  background: none;
  text-align: left;
}

.repo-value {
  overflow-wrap: anywhere;
  color: #d96200;
  font-family: ui-monospace, SFMono-Regular, Consolas, monospace;
  font-size: 12px;
  line-height: 1.5;
}

/* 窄屏下单选组排不进一行：允许换行，避免整页横向滚动 */
.block :deep(.n-radio-group) {
  display: flex;
  flex-wrap: wrap;
  gap: 6px 10px;
  height: auto;
  line-height: normal;
  white-space: normal;
}
</style>
