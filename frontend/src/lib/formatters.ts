/**
 * Strict binary volume formatting (1 KiB = 1024 B) vs decimal network bandwidth formatting (1 Kbps = 1000 bps).
 */

const BYTE_UNITS = ['B', 'KiB', 'MiB', 'GiB', 'TiB', 'PiB'] as const
const BPS_UNITS = ['bps', 'Kbps', 'Mbps', 'Gbps', 'Tbps'] as const

export function formatBytes(bytes: number): string {
  if (!Number.isFinite(bytes) || bytes <= 0) return '0 B'
  const exponent = Math.min(Math.floor(Math.log(bytes) / Math.log(1024)), BYTE_UNITS.length - 1)
  const value = bytes / Math.pow(1024, exponent)
  const unit = BYTE_UNITS[exponent] ?? 'B'
  return `${value >= 100 || exponent === 0 ? value.toFixed(0) : value.toFixed(2)} ${unit}`
}

export function formatBps(bps: number): string {
  if (!Number.isFinite(bps) || bps <= 0) return '0 bps'
  const exponent = Math.min(Math.floor(Math.log(bps) / Math.log(1000)), BPS_UNITS.length - 1)
  const value = bps / Math.pow(1000, exponent)
  const unit = BPS_UNITS[exponent] ?? 'bps'
  return `${value >= 100 || exponent === 0 ? value.toFixed(0) : value.toFixed(2)} ${unit}`
}

export function currentStatMonth(): string {
  const now = new Date()
  const year = now.getFullYear()
  const month = String(now.getMonth() + 1).padStart(2, '0')
  return `${year}-${month}`
}

export function formatDateTime(iso: string, locale: string): string {
  const date = new Date(iso)
  if (Number.isNaN(date.getTime())) return iso
  return new Intl.DateTimeFormat(locale, {
    year: 'numeric',
    month: '2-digit',
    day: '2-digit',
    hour: '2-digit',
    minute: '2-digit',
    second: '2-digit',
    hour12: false,
  }).format(date)
}

export function formatTimeShort(iso: string, locale: string): string {
  const date = new Date(iso)
  if (Number.isNaN(date.getTime())) return iso
  return new Intl.DateTimeFormat(locale, {
    hour: '2-digit',
    minute: '2-digit',
    hour12: false,
  }).format(date)
}
