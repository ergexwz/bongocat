import { invoke } from '@tauri-apps/api/core'
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

export type PanelKind = 'mail-list' | 'mail-archive' | 'mail-settings' | 'bubble'

const PANEL_SIZE: Record<Exclude<PanelKind, 'bubble'>, { width: number; height: number }> = {
  'mail-list': { width: 400, height: 560 },
  'mail-archive': { width: 400, height: 560 },
  'mail-settings': { width: 720, height: 720 },
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

export async function showAt(kind: PanelKind, cssHeight?: number) {
  const window = getCurrentWindow()
  const pet = await petBounds()
  const monitor = pet ? await monitorFor(pet) : (await availableMonitors())[0]
  const scale = monitor?.scaleFactor ?? pet?.scale ?? 1
  const base =
    kind === 'bubble'
      ? { width: 380, height: cssHeight ?? 120 }
      : PANEL_SIZE[kind]
  const areaPosition = monitor?.workArea.position ?? monitor?.position
  const areaSize = monitor?.workArea.size ?? monitor?.size
  let width = base.width
  let height = base.height

  // Keep the window inside the usable monitor area. Settings is the tallest
  // window and shrinks first on small/high-DPI displays; its content scrolls.
  if (kind !== 'bubble' && areaSize) {
    const availableWidth = Math.max(320, areaSize.width / scale - 64)
    const availableHeight = Math.max(360, areaSize.height / scale - 64)
    width = Math.min(width, availableWidth)
    height = Math.min(height, availableHeight)
  }

  const physicalWidth = Math.max(1, Math.round(width * scale))
  const physicalHeight = Math.max(1, Math.round(height * scale))

  let x = areaPosition ? areaPosition.x + 40 : 120
  let y = areaPosition ? areaPosition.y + 80 : 120
  if (pet) {
    if (kind === 'bubble') {
      x = pet.x + Math.round(pet.width / 2 - physicalWidth / 2)
      y = pet.y - physicalHeight - 8
    } else if (kind === 'mail-settings') {
      x = pet.x + Math.round(pet.width / 2 - physicalWidth / 2)
      y = pet.y + Math.round(pet.height / 2 - physicalHeight / 2)
    } else {
      x = pet.x + Math.round(pet.width / 2 - physicalWidth / 2)
      y = pet.y - physicalHeight - 8
      if (areaPosition && y < areaPosition.y) {
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
