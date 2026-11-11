// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (C) 2026 Lars Wallström

import { useState } from 'react';
import { useMutation, useQueryClient } from '@tanstack/react-query';
import { AlertTriangle } from 'lucide-react';
import {
  uploadModelDocument,
  type DocumentCategory,
} from '../../api/modelDocuments';
import styles from '../shared/RetireModal.module.css';

const MAX_UPLOAD_BYTES = 100 * 1024 * 1024;

const CATEGORY_LABELS: Record<DocumentCategory, string> = {
  instruction_manual: 'User Manual',
  maintenance_manual: 'Maintenance Manual',
  other: 'Other',
};

interface UploadDocumentModalProps {
  modelId: number;
  modelName: string;
  presetCategory?: DocumentCategory;
  isReplacing?: boolean;
  /// Only valid when presetCategory='other'. Targets a specific existing
  /// 'other' document for replacement (server unlinks the old file +
  /// audits as REPLACE_DOCUMENT). When undefined and presetCategory='other',
  /// the upload always inserts a new row.
  replaceDocId?: number;
  /// Pre-fill the title field. Used by the 'other' replace flow so the
  /// operator sees the existing title and can choose to keep or edit it.
  presetTitle?: string;
  onClose: () => void;
  onSuccess: () => void;
}

const MAX_TITLE_LEN = 200;

function formatMegabytes(bytes: number): string {
  return `${(bytes / (1024 * 1024)).toFixed(2)} MB`;
}

export function UploadDocumentModal({
  modelId,
  modelName,
  presetCategory,
  isReplacing = false,
  replaceDocId,
  presetTitle,
  onClose,
  onSuccess,
}: UploadDocumentModalProps) {
  const queryClient = useQueryClient();
  const [category, setCategory] = useState<DocumentCategory | ''>(
    presetCategory ?? '',
  );
  const [file, setFile] = useState<File | null>(null);
  const [title, setTitle] = useState<string>(presetTitle ?? '');
  const [error, setError] = useState<string | null>(null);

  const isOther = category === 'other';
  const trimmedTitle = title.trim();

  const mutation = useMutation<void, Error, void>({
    mutationFn: async () => {
      if (!category) throw new Error('Category is required');
      if (!file) throw new Error('File is required');
      if (file.size > MAX_UPLOAD_BYTES) {
        throw new Error(
          `File exceeds 100 MB limit (got ${formatMegabytes(file.size)})`,
        );
      }
      if (category === 'other') {
        if (trimmedTitle.length === 0) {
          throw new Error('Title is required for Other documents');
        }
        if (trimmedTitle.length > MAX_TITLE_LEN) {
          throw new Error(`Title must be at most ${MAX_TITLE_LEN} characters`);
        }
        await uploadModelDocument(modelId, category, file, {
          title: trimmedTitle,
          ...(replaceDocId !== undefined ? { replaceDocId } : {}),
        });
      } else {
        await uploadModelDocument(modelId, category, file);
      }
    },
    onSuccess: () => {
      queryClient.invalidateQueries({ queryKey: ['model-documents', modelId] });
      onSuccess();
      onClose();
    },
    onError: (e: Error) => setError(e.message),
  });

  const isReplaceMode = isReplacing || replaceDocId !== undefined;
  const submitLabel = isReplaceMode ? 'Replace' : 'Upload';
  const titleLabel = isReplaceMode ? 'Replace document' : 'Upload document';

  return (
    <div className={styles.overlay} onClick={onClose}>
      <div
        className={styles.modal}
        onClick={(e) => e.stopPropagation()}
        style={{ maxWidth: '480px' }}
      >
        <h2 className={styles.title}>{titleLabel}</h2>
        <p
          className={styles.body}
          style={{ marginBottom: 'var(--space-4)' }}
        >
          Model: <strong>«{modelName}»</strong>
        </p>

        {isReplaceMode && (
          <div
            style={{
              display: 'flex',
              alignItems: 'flex-start',
              gap: 'var(--space-2)',
              padding: 'var(--space-3)',
              marginBottom: 'var(--space-4)',
              background: 'color-mix(in srgb, var(--color-warning) 12%, transparent)',
              border: '1px solid color-mix(in srgb, var(--color-warning) 45%, transparent)',
              borderRadius: 'var(--radius-sm)',
              color: 'var(--color-text-primary)',
              fontSize: '0.85rem',
            }}
          >
            <AlertTriangle
              size={16}
              color="var(--color-warning)"
              style={{ flexShrink: 0, marginTop: 2 }}
            />
            <span>
              The existing document
              {presetTitle ? <> «<strong>{presetTitle}</strong>»</> : null} will
              be replaced. The old file is removed from disk and a single
              REPLACE_DOCUMENT audit entry records the previous filename, size,
              and title.
            </span>
          </div>
        )}

        <div style={{ marginBottom: 'var(--space-4)' }}>
          <label
            style={{
              display: 'block',
              fontSize: '0.8rem',
              color: 'var(--color-text-secondary)',
              fontFamily: 'var(--font-label)',
              marginBottom: 'var(--space-1)',
            }}
          >
            Category
          </label>
          <select
            value={category}
            onChange={(e) => setCategory(e.target.value as DocumentCategory | '')}
            disabled={presetCategory !== undefined || mutation.isPending}
            style={{
              background: 'var(--color-bg-input)',
              border: '1px solid var(--color-border)',
              borderRadius: 'var(--radius-sm)',
              padding: 'var(--space-2) var(--space-3)',
              color: 'var(--color-text-primary)',
              fontSize: '0.9rem',
              fontFamily: 'var(--font-body)',
              width: '100%',
            }}
          >
            <option value="">— Select category —</option>
            {Object.entries(CATEGORY_LABELS).map(([value, label]) => (
              <option key={value} value={value}>
                {label}
              </option>
            ))}
          </select>
        </div>

        {isOther && (
          <div style={{ marginBottom: 'var(--space-4)' }}>
            <label
              style={{
                display: 'block',
                fontSize: '0.8rem',
                color: 'var(--color-text-secondary)',
                fontFamily: 'var(--font-label)',
                marginBottom: 'var(--space-1)',
              }}
            >
              Title (required for Other)
            </label>
            <input
              type="text"
              value={title}
              onChange={(e) => setTitle(e.target.value)}
              placeholder="e.g. GPS Calibration Procedure"
              maxLength={MAX_TITLE_LEN}
              disabled={mutation.isPending}
              style={{
                background: 'var(--color-bg-input)',
                border: '1px solid var(--color-border)',
                borderRadius: 'var(--radius-sm)',
                padding: 'var(--space-2) var(--space-3)',
                color: 'var(--color-text-primary)',
                fontSize: '0.9rem',
                fontFamily: 'var(--font-body)',
                width: '100%',
              }}
            />
            <div
              style={{
                fontSize: '0.75rem',
                color: 'var(--color-text-muted)',
                marginTop: 'var(--space-1)',
              }}
            >
              {trimmedTitle.length} / {MAX_TITLE_LEN}
            </div>
          </div>
        )}

        <div style={{ marginBottom: 'var(--space-4)' }}>
          <label
            style={{
              display: 'block',
              fontSize: '0.8rem',
              color: 'var(--color-text-secondary)',
              fontFamily: 'var(--font-label)',
              marginBottom: 'var(--space-1)',
            }}
          >
            PDF file (max 100 MB)
          </label>
          <input
            type="file"
            accept="application/pdf,.pdf"
            disabled={mutation.isPending}
            onChange={(e) => {
              const f = e.target.files?.[0] ?? null;
              setFile(f);
              setError(null);
            }}
            style={{
              fontSize: '0.85rem',
              color: 'var(--color-text-primary)',
              width: '100%',
            }}
          />
          {file && (
            <div
              style={{
                fontSize: '0.8rem',
                color: 'var(--color-text-muted)',
                marginTop: 'var(--space-1)',
              }}
            >
              {file.name} — {formatMegabytes(file.size)}
            </div>
          )}
        </div>

        {error && (
          <p
            style={{
              color: 'var(--color-critical)',
              fontSize: '0.85rem',
              marginBottom: 'var(--space-4)',
              fontFamily: 'var(--font-body)',
            }}
          >
            {error}
          </p>
        )}

        <div className={styles.actions}>
          <button
            className={styles.btnCancel}
            onClick={onClose}
            disabled={mutation.isPending}
            type="button"
          >
            Cancel
          </button>
          <button
            className={isReplaceMode ? styles.btnRetire : styles.btnCancel}
            onClick={() => {
              setError(null);
              mutation.mutate();
            }}
            disabled={
              mutation.isPending ||
              !file ||
              !category ||
              (isOther && trimmedTitle.length === 0)
            }
            style={
              isReplaceMode
                ? undefined
                : {
                    background: 'var(--color-accent)',
                    color: 'var(--color-text-inverse)',
                    border: 'none',
                  }
            }
            type="button"
          >
            {mutation.isPending ? 'Uploading…' : submitLabel}
          </button>
        </div>
      </div>
    </div>
  );
}
