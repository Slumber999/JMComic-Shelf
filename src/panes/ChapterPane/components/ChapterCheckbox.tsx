import { defineComponent, type PropType, type Ref } from 'vue'
import { NCheckbox } from 'naive-ui'
import { ChapterInfo } from '../../../bindings.ts'

/// 章节复选框：勾选/取消勾选、框选高亮、禁用条件两个面板都一样，
/// 只有「状态色」不同（下载面板看下载状态，导出面板看导出结果），由 stateClass 传进来。
///
/// 用工厂函数而不是普通组件：勾选集是各自面板的 ref，必须闭包进来。
export function createChapterCheckbox(deps: {
  checkedIds: Ref<Set<number>>
  selectedIds: Ref<Set<number>>
  isChapterSelectable: (chapter: ChapterInfo) => boolean
  stateClass: (chapter: ChapterInfo) => Record<string, boolean>
}) {
  return defineComponent({
    name: 'ChapterCheckbox',
    props: {
      chapter: {
        type: Object as PropType<ChapterInfo>,
        required: true,
      },
    },
    setup(props) {
      return () => (
        <NCheckbox
          data-key={props.chapter.chapterId}
          class={[
            'hover:bg-gray-200!',
            {
              selectable: deps.isChapterSelectable(props.chapter),
              selected: deps.selectedIds.value.has(props.chapter.chapterId),
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
          label={props.chapter.chapterTitle}
          disabled={!deps.isChapterSelectable(props.chapter)}
        />
      )
    },
  })
}
