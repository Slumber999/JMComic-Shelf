import { ref } from 'vue'
import { useMessage } from 'naive-ui'
import { commands } from '../bindings.ts'
import type { Comic, LocalLibrarySource } from '../bindings.ts'
import { useI18n } from '../i18n.ts'

/// 手机端「导出 zip」：逐本打包，进度按本数报，完成后提示保存位置
/// - Android 10+：后端用 MediaStore 写进 下载/禁漫书架/，返回人类可读的 `Download/禁漫书架/{文件名}`
/// - 其它情况（桌面端 / Android 9 及以下 / MediaStore 失败）：后端返回 app 下载目录里的完整路径
export function useZipExport() {
  const { t } = useI18n()
  const message = useMessage()
  const exporting = ref(false)
  const done = ref(0)
  const total = ref(0)

  /// 完整路径 -> 所在目录：Win 和 Android 两种分隔符都认
  function folderOf(path: string): string {
    const index = Math.max(path.lastIndexOf('/'), path.lastIndexOf('\\'))
    return index > 0 ? path.slice(0, index) : path
  }

  async function exportZip(
    comics: Comic[],
    source: LocalLibrarySource,
  ): Promise<{ exported: number; failed: number }> {
    if (exporting.value || comics.length === 0) {
      return { exported: 0, failed: 0 }
    }
    exporting.value = true
    done.value = 0
    total.value = comics.length
    let exported = 0
    let failed = 0
    let lastPath = ''
    try {
      for (const comic of comics) {
        try {
          const result = await commands.exportComicZip(comic.id, source)
          if (result.status === 'error') {
            failed += 1
            console.error(result.error)
          } else {
            exported += 1
            lastPath = result.data
          }
        } catch (error) {
          failed += 1
          console.error(error)
        }
        done.value += 1
      }
    } finally {
      exporting.value = false
    }

    // 公共下载目录的返回值形如 `Download/禁漫书架/{文件名}`，其它情况按完整路径取目录
    const publicPrefix = 'Download/'
    const publicName = lastPath.startsWith(publicPrefix)
      ? lastPath.slice(publicPrefix.length)
      : ''

    const parts: string[] = []
    if (exported > 0) {
      parts.push(
        publicName
          ? t('mobile.zipExportedDownloads', { count: exported, name: publicName })
          : t('mobile.zipExported', { count: exported, path: folderOf(lastPath) }),
      )
    }
    if (failed > 0) {
      parts.push(t('mobile.zipFailed', { count: failed }))
    }
    if (parts.length === 0) {
      parts.push(t('mobile.batchFailed', { count: comics.length }))
    }
    const summary = parts.join('，')
    if (failed > 0) {
      message.warning(summary, { duration: 10000 })
    } else {
      message.success(summary, { duration: 10000 })
    }
    return { exported, failed }
  }

  return { exporting, done, total, exportZip }
}
