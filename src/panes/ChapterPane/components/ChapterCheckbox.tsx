import { defineComponent, type PropType, type Ref } from 'vue'
import { NCheckbox, NIcon } from 'naive-ui'
import { PhBookmarkSimple, PhCheck } from '@phosphor-icons/vue'
import { ChapterInfo } from '../../../bindings.ts'

/// 章节复选框：勾选/取消勾选、框选高亮、禁用条件两个面板都一样，
/// 只有「状态色」不同（下载面板看下载状态，导出面板看导出结果），由 stateClass 传进来。
/// 已读 / 当前正在读的标记也在这里统一画，两个面板不用各写一套。
///
/// 用工厂函数而不是普通组件：勾选集是各自面板的 ref，必须闭包进来。
export function createChapterCheckbox(deps: {
  checkedIds: Ref<Set<number>>
  selectedIds: Ref<Set<number>>
  isChapterSelectable: (chapter: ChapterInfo) => boolean
  stateClass: (chapter: ChapterInfo) => Record<string, boolean>
  /// 已读章节下标（和 chapterInfos 同序），只在集合变化时重算，模板里 O(1) 查
  readChapterIndexes: Ref<Set<number>>
  /// 当前读到的那章，-1 表示这本还没有进度
  readingChapterIndex: Ref<number>
}) {
  return defineComponent({
    name: 'ChapterCheckbox',
    props: {
      chapter: {
        type: Object as PropType<ChapterInfo>,
        required: true,
      },
      /// 章节在整本里的下标：和阅读器的 chapterIndex 是同一个
      index: {
        type: Number,
        required: true,
      },
    },
    setup(props) {
      return () => {
        const read = deps.readChapterIndexes.value.has(props.index)
        const reading = deps.readingChapterIndex.value === props.index
        return (
          <NCheckbox
            data-key={props.chapter.chapterId}
            class={[
              // 左侧 2px 留给「当前正在读」的色条，平时透明，不占位的写法会挤动这一格
              'hover:bg-gray-200! border-l-2 border-l-transparent',
              {
                selectable: deps.isChapterSelectable(props.chapter),
                selected: deps.selectedIds.value.has(props.chapter.chapterId),
                'border-l-jm-orange!': reading,
              },
              deps.stateClass(props.chapter),
            ]}
            checked={deps.checkedIds.value.has(props.chapter.chapterId)}
            onUpdate:checked={(checked: boolean) => {
              if (checked) {
                deps.checkedIds.value.add(props.chapter.chapterId)
              } else {
                deps.checkedIds.value.delete(props.chapter.chapterId)
              }
            }}
            disabled={!deps.isChapterSelectable(props.chapter)}>
            <span
              class={[
                'flex w-full min-w-0 items-center gap-1',
                // 已读用禁漫橙；当前章节再深一档并加粗，和普通已读区分
                reading ? 'text-jm-deep font-medium' : read ? 'text-jm-orange' : '',
              ]}>
              {read || reading ? (
                <NIcon size={12} class="shrink-0">
                  {reading ? <PhBookmarkSimple weight="fill" /> : <PhCheck />}
                </NIcon>
              ) : null}
              <span class="min-w-0 flex-1 truncate">{props.chapter.chapterTitle}</span>
            </span>
          </NCheckbox>
        )
      }
    },
  })
}
