// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (C) 2026 Lars Wallström

import JSZip from 'jszip';

export function filenameFromContentDisposition(header: string | null): string {
  if (!header) return `dronefleetpro_export_${Date.now()}.json`;
  const m = /filename="([^"]+)"/.exec(header);
  return m?.[1] ?? `dronefleetpro_export_${Date.now()}.json`;
}

export function downloadAsJson(blob: Blob, filename: string): void {
  const url = URL.createObjectURL(blob);
  triggerDownload(url, filename);
  URL.revokeObjectURL(url);
}

export async function downloadAsZip(jsonBlob: Blob, jsonFilename: string): Promise<void> {
  const zip = new JSZip();
  zip.file(jsonFilename, jsonBlob);
  const zipBlob = await zip.generateAsync({
    type: 'blob',
    compression: 'DEFLATE',
    compressionOptions: { level: 6 },
  });
  const zipFilename = jsonFilename.replace(/\.json$/, '.zip');
  const url = URL.createObjectURL(zipBlob);
  triggerDownload(url, zipFilename);
  URL.revokeObjectURL(url);
}

function triggerDownload(url: string, filename: string): void {
  const a = document.createElement('a');
  a.href = url;
  a.download = filename;
  document.body.appendChild(a);
  a.click();
  document.body.removeChild(a);
}

export function humaniseBytes(n: number): string {
  if (n < 1024) return `${n} bytes`;
  if (n < 1024 * 1024) return `${(n / 1024).toFixed(1)} KB`;
  return `${(n / (1024 * 1024)).toFixed(1)} MB`;
}

export function relativeTime(iso: string | null | undefined): string {
  if (!iso) return '—';
  const t = new Date(iso).getTime();
  const diff = Math.max(0, Date.now() - t);
  const sec = Math.floor(diff / 1000);
  if (sec < 60) return `${sec}s ago`;
  const min = Math.floor(sec / 60);
  if (min < 60) return `${min}m ago`;
  const hr = Math.floor(min / 60);
  if (hr < 24) return `${hr}h ago`;
  const day = Math.floor(hr / 24);
  return `${day}d ago`;
}
