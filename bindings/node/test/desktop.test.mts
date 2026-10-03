import assert from 'node:assert/strict'
import { test } from 'node:test'

import { arrange, Desktop, type Placement, type Rect } from '../index.js'

const bounds: Rect = { x: -20, y: 10, width: 101, height: 11 }

function throwsCode(fn: () => unknown, code: string): void {
  assert.throws(fn, (error: Error & { code?: string }) => error.code === code)
}

test('workspaces are validated and the first supplied ID is active', () => {
  throwsCode(() => new Desktop([]), 'NoWorkspaces')
  throwsCode(() => new Desktop([1, 1]), 'DuplicateWorkspace')
  throwsCode(() => new Desktop([0]), 'InvalidArg')
  throwsCode(() => new Desktop([1.5]), 'InvalidArg')
  throwsCode(() => new Desktop([2 ** 53]), 'InvalidArg')

  const desktop = new Desktop([2, 1])
  assert.equal(desktop.activeWorkspace, 2)
  assert.deepEqual(
    desktop.workspaces().map((ws) => [ws.id, ws.active]),
    [
      [1, false],
      [2, true],
    ],
  )
  assert.deepEqual(desktop.active(), {
    id: 2,
    windows: [],
    focused: null,
    layout: 'tall',
    active: true,
  })
})

test('membership is unique and windows keep tiling order', () => {
  const desktop = new Desktop([1, 2])
  for (const id of [3, 2, 1]) desktop.insert(id)
  assert.deepEqual(desktop.active().windows, [3, 1, 2])
  assert.equal(desktop.active().focused, 1)

  desktop.switchTo(2)
  throwsCode(() => desktop.insert(1), 'DuplicateWindow')
  throwsCode(() => desktop.switchTo(9), 'UnknownWorkspace')
  assert.equal(desktop.workspaceOf(1), 1)
  assert.equal(desktop.workspaceOf(42), null)

  desktop.moveWindow(1, 2)
  assert.deepEqual(desktop.active().windows, [1])
  assert.equal(desktop.workspaceOf(1), 2)

  desktop.remove(1)
  throwsCode(() => desktop.remove(1), 'UnknownWindow')
  assert.equal(desktop.active().focused, null)
})

test('focus wraps and promotion moves the focused window to the main pane', () => {
  const desktop = new Desktop([1])
  assert.equal(desktop.focusNext(), null)
  for (const id of [1, 2, 3]) desktop.insert(id)
  assert.equal(desktop.focusNext(), 1)
  assert.equal(desktop.focusPrevious(), 3)
  desktop.promoteFocused()
  assert.deepEqual(desktop.active().windows, [3, 1, 2])
  desktop.focus(2)
  throwsCode(() => desktop.focus(9), 'UnknownWindow')
  assert.equal(desktop.active().focused, 2)
})

test('tall and monocle placements match the Rust layout', () => {
  const desktop = new Desktop([1])
  for (const id of [1, 2, 3]) desktop.insert(id)
  const tall: Placement[] = desktop.arrange(bounds)
  assert.deepEqual(tall, [
    { window: 1, geometry: { x: -20, y: 10, width: 50, height: 11 } },
    { window: 2, geometry: { x: 30, y: 10, width: 51, height: 6 } },
    { window: 3, geometry: { x: 30, y: 16, width: 51, height: 5 } },
  ])

  desktop.setLayout('monocle')
  assert.equal(desktop.active().layout, 'monocle')
  assert.ok(desktop.arrange(bounds).every((p) => p.geometry.width === 101))

  assert.deepEqual(arrange('tall', bounds, [7]), [{ window: 7, geometry: bounds }])
  throwsCode(() => desktop.arrange({ ...bounds, width: 0 }), 'InvalidGeometry')
  throwsCode(() => arrange('tall', { ...bounds, width: 1 }, [1, 2]), 'InsufficientSpace')
})
