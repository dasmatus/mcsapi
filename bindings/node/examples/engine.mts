// A headless policy engine driven from TypeScript. A real compositor host would
// feed it surface lifecycle and key events, then configure surfaces from the
// placements; here a scripted event list stands in for both.
import { Desktop, type Placement, type Rect } from '../index.js'

type Event =
  | { kind: 'map'; window: number }
  | { kind: 'unmap'; window: number }
  | { kind: 'key'; binding: string }

const output: Rect = { x: 0, y: 0, width: 1920, height: 1080 }
const desktop = new Desktop([1, 2, 3, 4, 5, 6, 7, 8, 9])

const bindings: Record<string, () => void> = {
  'super+j': () => desktop.focusNext(),
  'super+k': () => desktop.focusPrevious(),
  'super+return': () => desktop.promoteFocused(),
  'super+space': () => desktop.setLayout(desktop.active().layout === 'tall' ? 'monocle' : 'tall'),
  'super+2': () => desktop.switchTo(2),
  'super+shift+2': () => {
    const focused = desktop.active().focused
    if (focused !== null) desktop.moveWindow(focused, 2)
  },
}

function apply(placements: Placement[]): void {
  for (const { window, geometry: g } of placements) {
    console.log(`  window ${window}: ${g.width}x${g.height} at ${g.x},${g.y}`)
  }
}

const events: Event[] = [
  { kind: 'map', window: 1 },
  { kind: 'map', window: 2 },
  { kind: 'map', window: 3 },
  { kind: 'key', binding: 'super+k' },
  { kind: 'key', binding: 'super+return' },
  { kind: 'key', binding: 'super+shift+2' },
  { kind: 'unmap', window: 2 },
  { kind: 'key', binding: 'super+space' },
]

for (const event of events) {
  if (event.kind === 'map') desktop.insert(event.window)
  else if (event.kind === 'unmap') desktop.remove(event.window)
  else bindings[event.binding]?.()
  const { id, focused, layout } = desktop.active()
  console.log(`${JSON.stringify(event)} -> workspace ${id}, ${layout}, focus ${focused}`)
  apply(desktop.arrange(output))
}
