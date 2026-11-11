// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (C) 2026 Lars Wallström

import JSZip from 'jszip';

const MAX_BYTES = 50 * 1024 * 1024; // 50MB — matches backend body limit

export interface ParsedDocument {
  json: unknown;
  filename: string;
  byteSize: number;
  wasZip: boolean;
}

export async function parseImportFile(file: File): Promise<ParsedDocument> {
  if (file.size > MAX_BYTES) {
    throw new Error(
      `File is too large (${(file.size / 1024 / 1024).toFixed(1)} MB). Maximum is 50 MB.`,
    );
  }

  const lower = file.name.toLowerCase();

  if (lower.endsWith('.zip')) {
    const zip = await JSZip.loadAsync(file);
    const jsonEntries = Object.values(zip.files).filter(
      (f) => !f.dir && f.name.toLowerCase().endsWith('.json'),
    );
    if (jsonEntries.length === 0) {
      throw new Error('ZIP archive contains no .json file.');
    }
    if (jsonEntries.length > 1) {
      throw new Error(
        `ZIP archive contains ${jsonEntries.length} .json files; expected exactly one.`,
      );
    }
    const text = await jsonEntries[0].async('string');
    if (text.length > MAX_BYTES) {
      throw new Error(
        `Extracted JSON is too large (${(text.length / 1024 / 1024).toFixed(1)} MB). Maximum is 50 MB.`,
      );
    }
    let parsed: unknown;
    try {
      parsed = JSON.parse(text);
    } catch (e) {
      throw new Error(`The file inside the ZIP is not valid JSON: ${(e as Error).message}`);
    }
    return {
      json: parsed,
      filename: jsonEntries[0].name,
      byteSize: text.length,
      wasZip: true,
    };
  }

  if (lower.endsWith('.json')) {
    const text = await file.text();
    let parsed: unknown;
    try {
      parsed = JSON.parse(text);
    } catch (e) {
      throw new Error(`The file is not valid JSON: ${(e as Error).message}`);
    }
    return {
      json: parsed,
      filename: file.name,
      byteSize: file.size,
      wasZip: false,
    };
  }

  throw new Error('Unsupported file type. Please upload a .json or .zip file.');
}

export function describeDocument(doc: unknown): {
  formatVersion: number | null;
  sourceOrgId: number | null;
  tableCount: number;
} {
  if (!doc || typeof doc !== 'object') {
    return { formatVersion: null, sourceOrgId: null, tableCount: 0 };
  }
  const o = doc as Record<string, unknown>;
  const formatVersion = typeof o.format_version === 'number' ? o.format_version : null;
  const sourceOrgId = typeof o.source_org_id === 'number' ? o.source_org_id : null;
  const tables =
    o.tables && typeof o.tables === 'object' ? (o.tables as Record<string, unknown>) : {};
  return {
    formatVersion,
    sourceOrgId,
    tableCount: Object.keys(tables).length,
  };
}
