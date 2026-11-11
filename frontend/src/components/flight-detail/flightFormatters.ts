// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (C) 2026 Lars Wallström

// Shared formatters for the flight detail page subcomponents.
// Each helper falls back gracefully on missing/null data — callers can
// pass a value directly without pre-checking.

export function formatDuration(seconds: number | null | undefined): string {
  if (seconds == null || seconds <= 0) return '—';
  if (seconds < 3600) {
    const m = Math.floor(seconds / 60);
    const s = seconds % 60;
    return `${m}m ${String(s).padStart(2, '0')}s`;
  }
  const h = Math.floor(seconds / 3600);
  const m = Math.floor((seconds % 3600) / 60);
  return `${h}h ${String(m).padStart(2, '0')}m`;
}

export function formatDate(iso: string | null | undefined): string {
  if (!iso) return '—';
  return iso.substring(0, 10);
}

export function formatDateTime(iso: string | null | undefined): string {
  if (!iso) return '—';
  // ISO 8601 in UTC, displayed as YYYY-MM-DD HH:MM UTC
  return `${iso.substring(0, 10)} ${iso.substring(11, 16)} UTC`;
}

export function formatTime(iso: string | null | undefined): string {
  if (!iso) return '—';
  return `${iso.substring(11, 16)} UTC`;
}

export function formatCoords(
  lat: number | null | undefined,
  lon: number | null | undefined
): string {
  if (lat == null || lon == null) return '—';
  const latDir = lat >= 0 ? 'N' : 'S';
  const lonDir = lon >= 0 ? 'E' : 'W';
  return `${Math.abs(lat).toFixed(5)}°${latDir}, ${Math.abs(lon).toFixed(5)}°${lonDir}`;
}

export function formatNumber(
  v: number | null | undefined,
  decimals = 1,
  unit = ''
): string {
  if (v == null) return '—';
  return `${v.toFixed(decimals)}${unit ? ` ${unit}` : ''}`;
}

export function sourceLabel(source: string | null | undefined): string {
  if (!source) return 'Unknown';
  if (source === 'dji_import') return 'DJI Import';
  if (source === 'csv_import') return 'CSV Import';
  if (source === 'manual') return 'Manual';
  return source;
}
