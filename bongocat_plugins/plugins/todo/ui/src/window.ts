import { invoke } from '@tauri-apps/api/core'
import { getCurrentWebview } from '@tauri-apps/api/webview'
import {
  PhysicalPosition,
  PhysicalSize,
  availableMonitors,
  getCurrentWindow,
} from '@tauri-apps/api/window'

export interface PetBounds {
  x: number
  y: number
  width: number
  height: number
  scale: number
}

export type PanelKind = 'todo' | 'todo-mini' | 'bubble'

export const PANEL_SIZE: Record<Exclude<PanelKind, 'bubble'>, { width: number; height: number }> = {
  todo: { width: 380, height: 560 },
  'todo-mini': { width: 380, height: 130 },
}

export async function petBounds(): Promise<PetBounds | null> {
  return invoke<PetBounds | null>('get_pet_bounds')
}

async function monitorFor(pet: PetBounds) {
  const monitors = await availableMonitors()
  return (
    monitors.find(
      (monitor) =>
        pet.x >= monitor.position.x &&
        pet.x < monitor.position.x + monitor.size.width &&
        pet.y >= monitor.position.y &&
        pet.y < monitor.position.y + monitor.size.height,
    ) ?? monitors[0]
  )
}

export async function showAt(kind: PanelKind, cssHeight?: number, uiScale = 1) {
  const window = getCurrentWindow()
  const pet = await petBounds()
  const monitor = pet ? await monitorFor(pet) : (await availableMonitors())[0]
  const scale = monitor?.scaleFactor ?? pet?.scale ?? 1
  const base =
    kind === 'bubble'
      ? { width: 380, height: cssHeight ?? 120 }
      : {
          width: PANEL_SIZE[kind].width * uiScale,
          height: PANEL_SIZE[kind].height * uiScale,
        }
  const physicalWidth = Math.round(base.width * scale)
  const physicalHeight = Math.round(base.height * scale)

  const areaPosition = monitor?.workArea.position ?? monitor?.position
  const areaSize = monitor?.workArea.size ?? monitor?.size
  let x = areaPosition ? areaPosition.x + 40 : 120
  let y = areaPosition ? areaPosition.y + 80 : 120
  if (pet) {
    if (kind === 'todo-mini') {
      x = pet.x + Math.round(pet.width * 0.6)
      y = pet.y - physicalHeight - 8
      if (y < (areaPosition?.y ?? 0)) {
        y = pet.y + pet.height + 8
      }
    } else {
      x = pet.x + Math.round(pet.width / 2 - physicalWidth / 2)
      y = pet.y - physicalHeight - 8
      if (kind === 'todo' && areaPosition && y < areaPosition.y) {
        y = pet.y + pet.height + 8
      }
    }
  }
  if (areaPosition && areaSize) {
    x = Math.max(
      areaPosition.x,
      Math.min(x, areaPosition.x + areaSize.width - physicalWidth),
    )
    y = Math.max(
      areaPosition.y,
      Math.min(y, areaPosition.y + areaSize.height - physicalHeight),
    )
  }

  await window.setSize(new PhysicalSize(physicalWidth, physicalHeight))
  await window.setPosition(new PhysicalPosition(Math.round(x), Math.round(y)))
  if (kind !== 'bubble') {
    // Native WebView zoom keeps text crisp and reflows the UI to the window size.
    await getCurrentWebview().setZoom(uiScale).catch((error) => {
      console.error('could not apply todo zoom', error)
    })
  }
  await window.show()
  if (kind !== 'bubble') {
    await window.setFocus()
  }
}

export async function hideCurrent() {
  await getCurrentWindow().hide()
}

export async function currentVisible() {
  return getCurrentWindow().isVisible()
}
