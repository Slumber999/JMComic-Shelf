import { commands } from './bindings.ts'
import { showError } from './errors.ts'
import { useStore } from './store.ts'

/// 打开漫画详情页的唯一入口：先按 id 取完整详情再切换页签。
/// 只传 id/名字/作者就切页签会让详情页缺章节和标签——所有入口都必须走这里。
/// 失败在这里统一弹一次通知：调用方只关心成功后的事，不用各自处理错误。
export async function openComicDetail(comicId: number): Promise<boolean> {
  const store = useStore()
  const result = await commands.getComic(comicId)
  if (result.status === 'error') {
    showError(result.error)
    return false
  }
  store.pickComic(result.data)
  return true
}
