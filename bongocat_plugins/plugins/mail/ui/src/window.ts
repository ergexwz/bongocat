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

export type PanelKind = 'mail-list' | 'mail-archive' | 'mail-settings' | 'bubble'

export const MIN_UI_SCALE = 0.8
export const MAX_UI_SCALE = 1.5

export const PANEL_SIZE: Record<Exclude<PanelKind, 'bubble'>, { width: number; height: number }> = {
  'mail-list': { width: 400, height: 560 },
  'mail-archive': { width: 400, height: 560 },
  'mail-settings': { width: 720, height: 720 },
}

export function clampUiScale(scale: number) {
  return Math.min(MAX_UI_SCALE, Math.max(MIN_UI_SCALE, Math.round(scale * 1000) / 1000))
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
  const dpiScale = monitor?.scaleFactor ?? pet?.scale ?? 1
  const base = kind === 'bubble'
    ? { width: 380, height: cssHeight ?? 120 }
    : PANEL_SIZE[kind]
  const isBubble = kind === 'bubble'
  const areaPosition = monitor?.workArea.position ?? monitor?.position
  const areaSize = monitor?.workArea.size ?? monitor?.size
  let width = base.width
  let height = base.height
  let appliedScale = 1

  if (isBubble) {
    // Bubbles size to their content and keep the existing independent clamp.
    if (areaSize) {
      const availableWidth = Math.max(320, areaSize.width / dpiScale - 64)
      const availableHeight = Math.max(360, areaSize.height / dpiScale - 64)
      width = Math.min(width, availableWidth)
      height = Math.min(height, availableHeight)
    }
  } else {
    // One scale for both dimensions keeps the window's aspect ratio fixed. If
    // the requested scale would not fit the monitor's usable area, shrink the
    // same factor on both axes instead of clamping width and height apart.
    const requestedScale = clampUiScale(uiScale)
    width = base.width * requestedScale
    height = base.height * requestedScale
    let fit = 1
    if (areaSize) {
      const availableWidth = Math.max(1, areaSize.width / dpiScale - 64)
      const availableHeight = Math.max(1, areaSize.height / dpiScale - 64)
      fit = Math.min(1, availableWidth / width, availableHeight / height)
    }
    appliedScale = requestedScale * fit
    width = base.width * appliedScale
    height = base.height * appliedScale
  }

  const physicalWidth = Math.max(1, Math.round(width * dpiScale))
  const physicalHeight = Math.max(1, Math.round(height * dpiScale))

  let x = areaPosition ? areaPosition.x + 40 : 120
  let y = areaPosition ? areaPosition.y + 80 : 120
  if (pet) {
    if (isBubble) {
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
  if (!isBubble) {
    // Native WebView zoom keeps text crisp; the window is sized for the same
    // scale so the CSS layout viewport stays at the panel's base dimensions.
    await getCurrentWebview().setZoom(appliedScale).catch((error) => {
      console.error('could not apply mail zoom', error)
    })
  }
  await window.show()
  if (!isBubble) {
    await window.setFocus()
  }
}

export async function hideCurrent() {
  await getCurrentWindow().hide()
}

export async function currentVisible() {
  return getCurrentWindow().isVisible()
}
