// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (C) 2026 Lars Wallström

import { useState } from 'react';
import { useMutation, useQueryClient } from '@tanstack/react-query';
import { Download, Eye, FileText, Plus, Replace, Trash2, Upload } from 'lucide-react';
import {
  deleteModelDocument,
  downloadModelDocument,
  type DocumentCategory,
  type ModelDocument,
} from '../../api/modelDocuments';
import { useModelDocuments } from '../../hooks/useFleetStats';
import { useCurrentUser } from '../../hooks/useCurrentUser';
import { userHasAnyRole } from '../../utils/auth';
import { PdfViewer } from '../PdfViewer';
import { UploadDocumentModal } from './UploadDocumentModal';
import retireStyles from '../shared/RetireModal.module.css';
import styles from '../../pages/FleetStatsShared.module.css';

interface DocumentsSectionProps {
  modelId: number;
  modelName: string;
}

const CATEGORY_LABELS: Record<DocumentCategory, string> = {
  instruction_manual: 'User Manual',
  maintenance_manual: 'Maintenance Manual',
  other: 'Other',
};

type ManualCategory = 'instruction_manual' | 'maintenance_manual';
const MANUAL_CATEGORIES: ManualCategory[] = ['instruction_manual', 'maintenance_manual'];

/// Upload intents the modal can be opened in. The discriminated union keeps
/// the manual/other branches type-safe and lets us pass exactly the props
/// each branch needs.
type UploadIntent =
  | { kind: 'upload-manual'; category: ManualCategory }
  | { kind: 'replace-manual'; category: ManualCategory }
  | { kind: 'upload-other' }
  | { kind: 'replace-other'; docId: number; title: string };

function formatMegabytes(bytes: number): string {
  return `${(bytes / (1024 * 1024)).toFixed(2)} MB`;
}

function truncateFilename(name: string, max = 48): string {
  if (name.length <= max) return name;
  return name.slice(0, max - 1) + '…';
}

export function DocumentsSection({ modelId, modelName }: DocumentsSectionProps) {
  const { data: currentUser } = useCurrentUser();
  const isAdmin = userHasAnyRole(currentUser, 'admin');

  const { data, isLoading, error } = useModelDocuments(modelId);
  const [intent, setIntent] = useState<UploadIntent | null>(null);
  const [pendingDelete, setPendingDelete] = useState<ModelDocument | null>(null);
  const [actionError, setActionError] = useState<string | null>(null);
  const [viewerDoc, setViewerDoc] = useState<{ url: string; filename: string } | null>(null);

  // Group: at most one manual per category, but any number of 'other' rows.
  const manualByCategory: Partial<Record<ManualCategory, ModelDocument>> = {};
  const otherDocs: ModelDocument[] = [];
  for (const doc of data ?? []) {
    if (doc.category === 'other') {
      otherDocs.push(doc);
    } else if (doc.category === 'instruction_manual' || doc.category === 'maintenance_manual') {
      manualByCategory[doc.category] = doc;
    }
  }

  const handleDownload = async (doc: ModelDocument) => {
    setActionError(null);
    try {
      await downloadModelDocument(modelId, doc.id, doc.original_filename);
    } catch (e) {
      setActionError(`Download failed: ${(e as Error).message}`);
    }
  };

  const handleView = (doc: ModelDocument) => {
    setActionError(null);
    setViewerDoc({
      url: `/api/v1/aircraft-models/${modelId}/documents/${doc.id}/download`,
      filename: doc.original_filename,
    });
  };

  return (
    <section className={styles.section}>
      <h2 className={styles.sectionTitle}>Documents</h2>

      {isLoading ? (
        <p style={{ color: 'var(--color-text-secondary)' }}>Loading documents…</p>
      ) : error ? (
        <p style={{ color: 'var(--color-critical)' }}>
          Failed to load documents: {(error as Error).message}
        </p>
      ) : (
        <div
          style={{
            display: 'flex',
            flexDirection: 'column',
            gap: 'var(--space-3)',
          }}
        >
          {MANUAL_CATEGORIES.map((cat) => (
            <ManualSlot
              key={cat}
              category={cat}
              document={manualByCategory[cat]}
              isAdmin={isAdmin}
              onUploadClick={() => setIntent({ kind: 'upload-manual', category: cat })}
              onReplaceClick={() => setIntent({ kind: 'replace-manual', category: cat })}
              onViewClick={() => manualByCategory[cat] && handleView(manualByCategory[cat]!)}
              onDownloadClick={() =>
                manualByCategory[cat] && handleDownload(manualByCategory[cat]!)
              }
              onDeleteClick={() =>
                manualByCategory[cat] && setPendingDelete(manualByCategory[cat]!)
              }
            />
          ))}

          <OtherDocumentsGroup
            documents={otherDocs}
            isAdmin={isAdmin}
            onUploadAnother={() => setIntent({ kind: 'upload-other' })}
            onReplace={(doc) =>
              setIntent({
                kind: 'replace-other',
                docId: doc.id,
                title: doc.title ?? '',
              })
            }
            onView={(doc) => handleView(doc)}
            onDownload={(doc) => handleDownload(doc)}
            onDelete={(doc) => setPendingDelete(doc)}
          />
        </div>
      )}

      {actionError && (
        <p
          style={{
            color: 'var(--color-critical)',
            fontSize: '0.85rem',
            marginTop: 'var(--space-2)',
          }}
        >
          {actionError}
        </p>
      )}

      {intent && (
        <UploadDocumentModal
          modelId={modelId}
          modelName={modelName}
          presetCategory={
            intent.kind === 'upload-manual' || intent.kind === 'replace-manual'
              ? intent.category
              : 'other'
          }
          isReplacing={intent.kind === 'replace-manual'}
          replaceDocId={intent.kind === 'replace-other' ? intent.docId : undefined}
          presetTitle={intent.kind === 'replace-other' ? intent.title : undefined}
          onClose={() => setIntent(null)}
          onSuccess={() => {
            // Hook already invalidates the query; no extra work here.
          }}
        />
      )}

      {pendingDelete && (
        <DeleteDocumentModal
          modelId={modelId}
          document={pendingDelete}
          onClose={() => setPendingDelete(null)}
        />
      )}

      {viewerDoc && (
        <PdfViewer
          url={viewerDoc.url}
          filename={viewerDoc.filename}
          onClose={() => setViewerDoc(null)}
        />
      )}
    </section>
  );
}

// ---- Shared inline styles ----

const slotStyle: React.CSSProperties = {
  display: 'flex',
  alignItems: 'center',
  gap: 'var(--space-4)',
  padding: 'var(--space-3) var(--space-4)',
  background: 'var(--color-bg-surface)',
  border: '1px solid var(--color-border)',
  borderRadius: 'var(--radius-md)',
};

const categoryLabelStyle: React.CSSProperties = {
  fontFamily: 'var(--font-label)',
  fontSize: '0.75rem',
  color: 'var(--color-text-muted)',
  textTransform: 'uppercase',
  letterSpacing: '0.05em',
  minWidth: '170px',
};

const inlineBtnStyle: React.CSSProperties = {
  background: 'transparent',
  border: '1px solid var(--color-border)',
  borderRadius: 'var(--radius-sm)',
  color: 'var(--color-text-secondary)',
  padding: 'var(--space-1) var(--space-3)',
  fontSize: '0.8rem',
  cursor: 'pointer',
  display: 'inline-flex',
  alignItems: 'center',
  gap: 'var(--space-1)',
};

const primaryBtnStyle: React.CSSProperties = {
  ...inlineBtnStyle,
  background: 'var(--color-accent)',
  color: 'var(--color-text-inverse)',
  border: '1px solid var(--color-accent)',
};

const dangerBtnStyle: React.CSSProperties = {
  ...inlineBtnStyle,
  color: 'var(--color-critical)',
  borderColor: 'color-mix(in srgb, var(--color-critical) 40%, transparent)',
};

const otherGroupStyle: React.CSSProperties = {
  display: 'flex',
  flexDirection: 'column',
  gap: 'var(--space-2)',
  padding: 'var(--space-3) var(--space-4)',
  background: 'var(--color-bg-surface)',
  border: '1px solid var(--color-border)',
  borderRadius: 'var(--radius-md)',
};

// ---- Manual slot (instruction_manual / maintenance_manual) ----

interface ManualSlotProps {
  category: ManualCategory;
  document: ModelDocument | undefined;
  isAdmin: boolean;
  onUploadClick: () => void;
  onReplaceClick: () => void;
  onViewClick: () => void;
  onDownloadClick: () => void;
  onDeleteClick: () => void;
}

function ManualSlot({
  category,
  document,
  isAdmin,
  onUploadClick,
  onReplaceClick,
  onViewClick,
  onDownloadClick,
  onDeleteClick,
}: ManualSlotProps) {
  return (
    <div style={slotStyle}>
      <div style={categoryLabelStyle}>{CATEGORY_LABELS[category]}</div>
      {document ? (
        <>
          <div style={{ flex: 1, minWidth: 0 }}>
            <div
              style={{
                display: 'flex',
                alignItems: 'center',
                gap: 'var(--space-2)',
              }}
              title={document.original_filename}
            >
              <FileText size={14} style={{ color: 'var(--color-text-muted)', flexShrink: 0 }} />
              <span
                style={{
                  fontSize: '0.9rem',
                  color: 'var(--color-text-primary)',
                  overflow: 'hidden',
                  textOverflow: 'ellipsis',
                  whiteSpace: 'nowrap',
                }}
              >
                {truncateFilename(document.original_filename)}
              </span>
            </div>
            <div
              style={{
                fontSize: '0.75rem',
                color: 'var(--color-text-muted)',
                marginTop: 'var(--space-1)',
              }}
            >
              {formatMegabytes(document.size_bytes)} · uploaded{' '}
              {document.uploaded_at.substring(0, 10)} · by user #
              {document.uploaded_by}
            </div>
          </div>
          <div style={{ display: 'flex', gap: 'var(--space-2)', flexShrink: 0 }}>
            <button
              type="button"
              style={inlineBtnStyle}
              onClick={onViewClick}
              title="View PDF"
            >
              <Eye size={14} /> View
            </button>
            <button
              type="button"
              style={inlineBtnStyle}
              onClick={onDownloadClick}
              title="Download PDF"
            >
              <Download size={14} /> Download
            </button>
            {isAdmin && (
              <>
                <button
                  type="button"
                  style={inlineBtnStyle}
                  onClick={onReplaceClick}
                  title="Replace this document"
                >
                  <Replace size={14} /> Replace
                </button>
                <button
                  type="button"
                  style={dangerBtnStyle}
                  onClick={onDeleteClick}
                  title="Delete this document"
                >
                  <Trash2 size={14} /> Delete
                </button>
              </>
            )}
          </div>
        </>
      ) : (
        <>
          <div
            style={{
              flex: 1,
              fontSize: '0.85rem',
              color: 'var(--color-text-muted)',
              fontStyle: 'italic',
            }}
          >
            No document uploaded.
          </div>
          {isAdmin && (
            <button
              type="button"
              style={primaryBtnStyle}
              onClick={onUploadClick}
              title={`Upload ${CATEGORY_LABELS[category]}`}
            >
              <Upload size={14} /> Upload
            </button>
          )}
        </>
      )}
    </div>
  );
}

// ---- Other documents group ----

interface OtherDocumentsGroupProps {
  documents: ModelDocument[];
  isAdmin: boolean;
  onUploadAnother: () => void;
  onReplace: (doc: ModelDocument) => void;
  onView: (doc: ModelDocument) => void;
  onDownload: (doc: ModelDocument) => void;
  onDelete: (doc: ModelDocument) => void;
}

function OtherDocumentsGroup({
  documents,
  isAdmin,
  onUploadAnother,
  onReplace,
  onView,
  onDownload,
  onDelete,
}: OtherDocumentsGroupProps) {
  return (
    <div style={otherGroupStyle}>
      <div style={{ display: 'flex', alignItems: 'center', gap: 'var(--space-4)' }}>
        <div style={{ ...categoryLabelStyle, minWidth: 'auto' }}>Other documents</div>
        <div style={{ flex: 1 }} />
        {isAdmin && (
          <button
            type="button"
            style={primaryBtnStyle}
            onClick={onUploadAnother}
            title="Upload another other document"
          >
            <Plus size={14} /> Upload another
          </button>
        )}
      </div>

      {documents.length === 0 ? (
        <div
          style={{
            fontSize: '0.85rem',
            color: 'var(--color-text-muted)',
            fontStyle: 'italic',
          }}
        >
          No other documents uploaded.
        </div>
      ) : (
        <div
          style={{
            display: 'flex',
            flexDirection: 'column',
            gap: 'var(--space-2)',
          }}
        >
          {documents.map((doc) => (
            <OtherDocumentRow
              key={doc.id}
              document={doc}
              isAdmin={isAdmin}
              onReplace={() => onReplace(doc)}
              onView={() => onView(doc)}
              onDownload={() => onDownload(doc)}
              onDelete={() => onDelete(doc)}
            />
          ))}
        </div>
      )}
    </div>
  );
}

interface OtherDocumentRowProps {
  document: ModelDocument;
  isAdmin: boolean;
  onReplace: () => void;
  onView: () => void;
  onDownload: () => void;
  onDelete: () => void;
}

const otherRowStyle: React.CSSProperties = {
  display: 'flex',
  alignItems: 'center',
  gap: 'var(--space-4)',
  padding: 'var(--space-2) var(--space-3)',
  background: 'var(--color-bg-elevated)',
  border: '1px solid var(--color-border)',
  borderRadius: 'var(--radius-sm)',
};

function OtherDocumentRow({
  document,
  isAdmin,
  onReplace,
  onView,
  onDownload,
  onDelete,
}: OtherDocumentRowProps) {
  const title = document.title ?? document.original_filename;
  return (
    <div style={otherRowStyle}>
      <div style={{ flex: 1, minWidth: 0 }}>
        <div
          style={{
            display: 'flex',
            alignItems: 'center',
            gap: 'var(--space-2)',
          }}
          title={title}
        >
          <FileText size={14} style={{ color: 'var(--color-text-muted)', flexShrink: 0 }} />
          <span
            style={{
              fontSize: '0.9rem',
              color: 'var(--color-text-primary)',
              overflow: 'hidden',
              textOverflow: 'ellipsis',
              whiteSpace: 'nowrap',
              fontWeight: 500,
            }}
          >
            {title}
          </span>
        </div>
        <div
          style={{
            fontSize: '0.75rem',
            color: 'var(--color-text-muted)',
            marginTop: 'var(--space-1)',
          }}
          title={document.original_filename}
        >
          {truncateFilename(document.original_filename)} ·{' '}
          {formatMegabytes(document.size_bytes)} · uploaded{' '}
          {document.uploaded_at.substring(0, 10)} · by user #{document.uploaded_by}
        </div>
      </div>
      <div style={{ display: 'flex', gap: 'var(--space-2)', flexShrink: 0 }}>
        <button type="button" style={inlineBtnStyle} onClick={onView} title="View PDF">
          <Eye size={14} /> View
        </button>
        <button type="button" style={inlineBtnStyle} onClick={onDownload} title="Download PDF">
          <Download size={14} /> Download
        </button>
        {isAdmin && (
          <>
            <button
              type="button"
              style={inlineBtnStyle}
              onClick={onReplace}
              title="Replace this document"
            >
              <Replace size={14} /> Replace
            </button>
            <button
              type="button"
              style={dangerBtnStyle}
              onClick={onDelete}
              title="Delete this document"
            >
              <Trash2 size={14} /> Delete
            </button>
          </>
        )}
      </div>
    </div>
  );
}

// ---- Delete confirmation modal ----

interface DeleteDocumentModalProps {
  modelId: number;
  document: ModelDocument;
  onClose: () => void;
}

function DeleteDocumentModal({
  modelId,
  document,
  onClose,
}: DeleteDocumentModalProps) {
  const queryClient = useQueryClient();
  const [error, setError] = useState<string | null>(null);

  const mutation = useMutation<void, Error, void>({
    mutationFn: async () => {
      await deleteModelDocument(modelId, document.id);
    },
    onSuccess: () => {
      queryClient.invalidateQueries({ queryKey: ['model-documents', modelId] });
      onClose();
    },
    onError: (e: Error) => setError(e.message),
  });

  const displayLabel = document.title ?? document.original_filename;

  return (
    <div className={retireStyles.overlay} onClick={onClose}>
      <div
        className={retireStyles.modal}
        onClick={(e) => e.stopPropagation()}
      >
        <h2 className={retireStyles.title}>Delete document</h2>
        <p className={retireStyles.body}>
          This will permanently delete the document{' '}
          <strong>«{displayLabel}»</strong> ({CATEGORY_LABELS[document.category]}).
          The file is removed from disk and a DELETE_DOCUMENT audit row is
          written. This action cannot be undone.
        </p>
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
        <div className={retireStyles.actions}>
          <button
            className={retireStyles.btnCancel}
            onClick={onClose}
            disabled={mutation.isPending}
            type="button"
          >
            Cancel
          </button>
          <button
            className={retireStyles.btnRetire}
            onClick={() => {
              setError(null);
              mutation.mutate();
            }}
            disabled={mutation.isPending}
            type="button"
          >
            {mutation.isPending ? 'Deleting…' : 'Delete'}
          </button>
        </div>
      </div>
    </div>
  );
}
