// 返回键优先级的状态机单测：纯逻辑，不碰 DOM / history。
// 跑法（Node >= 22.6）：node --experimental-strip-types --test src/mobile/backPriority.test.mjs
import test from 'node:test'
import assert from 'node:assert/strict'
import { nextBackAction } from './backPriority.ts'

test('选中模式：返回只清选中，不切屏、不退出', () => {
  const action = nextBackAction({ selection: true, atRoot: false, search: true, exitHint: true })
  assert.equal(action, 'selection')
  // 既不是切屏也不是退出：屏幕栈与历史条目都不该动
  assert.notEqual(action, 'root')
  assert.notEqual(action, 'exit')
})

test('收藏夹选中态连按三次：退出选中 → 回搜索页 → 提示退出', () => {
  // 第 1 次：收藏夹里选中，只清选中
  const first = nextBackAction({ selection: true, atRoot: false, search: false, exitHint: false })
  // 第 2 次：选中已清，仍在收藏夹 → 回搜索根屏
  const second = nextBackAction({ selection: false, atRoot: false, search: false, exitHint: false })
  // 第 3 次：已在搜索根屏空态 → 提示
  const third = nextBackAction({ selection: false, atRoot: true, search: false, exitHint: false })
  // 第 4 次：提示还在 → 退出
  const fourth = nextBackAction({ selection: false, atRoot: true, search: false, exitHint: true })
  assert.deepEqual([first, second, third, fourth], ['selection', 'root', 'hint', 'exit'])
})

test('首页搜索态：一次返回清空到空态，再返回才是提示', () => {
  const first = nextBackAction({ selection: false, atRoot: true, search: true, exitHint: false })
  // 清空后首页已是空态（search=false）
  const second = nextBackAction({ selection: false, atRoot: true, search: false, exitHint: false })
  const third = nextBackAction({ selection: false, atRoot: true, search: false, exitHint: true })
  assert.deepEqual([first, second, third], ['search', 'hint', 'exit'])
})

test('在别的根屏时先回搜索页，不会把搜索态当成这一层吃掉', () => {
  const action = nextBackAction({ selection: false, atRoot: false, search: true, exitHint: false })
  assert.equal(action, 'root')
})

test('优先级固定：选中模式胜过搜索态与退出提示', () => {
  assert.equal(nextBackAction({ selection: true, atRoot: true, search: true, exitHint: true }), 'selection')
  assert.equal(nextBackAction({ selection: false, atRoot: true, search: true, exitHint: true }), 'search')
  assert.equal(nextBackAction({ selection: false, atRoot: true, search: false, exitHint: true }), 'exit')
})

test('同一状态永远得到同一动作（不依赖时序）', () => {
  const state = { selection: false, atRoot: false, search: false, exitHint: false }
  assert.equal(nextBackAction(state), nextBackAction(state))
  assert.equal(nextBackAction(state), 'root')
})
