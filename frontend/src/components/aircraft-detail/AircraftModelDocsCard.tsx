// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (C) 2026 Lars Wallström

import { useState } from 'react';
import { Link } from 'react-router-dom';
import { Download, Eye } from 'lucide-react';
import { useModelDocuments, useModelChecklists } from '../../hooks/useFleetStats';
import {
  downloadModelDocument,
  type DocumentCategory,
  type ModelDocument,
} from '../../api/modelDocuments';
import type { ChecklistSummary } from '../../api/checklists';
import { PdfViewer } from '../PdfViewer';
import styles from './AircraftModelDocsCard.module.css';

interface AircraftModelDocsCardProps {
  model: { id: number; name: string } | null;
}

const CATEGORY_LABEL: Record<DocumentCategory, string> = {
  instruction_manual: 'User Manual',
  maintenance_manual: 'Maintenance manual',
  other: 'Other',
};

const CATEGORY_BADGE_CLASS: Record<DocumentCategory, string> = {
  instruction_manual: styles.badgeInstruction,
  maintenance_manual: styles.badgeMaintenance,
  other: styles.badgeOther,
};

const CATEGORY_BADGE_SHORT: Record<DocumentCategory, string> = {
  instruction_manual: 'User',
  maintenance_manual: 'Maintenance',
  other: 'Other',
};

/// Compact embed of the model's documents and checklists on AircraftDetailPage.
/// Reuses the model-scoped hooks introduced by PR #79 (documents) and PR #81
/// (checklists). Returns null when the aircraft is not linked to a model so
/// the page stays clean for legacy rows.
export function AircraftModelDocsCard({ model }: AircraftModelDocsCardProps) {
  if (!model) return null;
  return <CardBody modelId={model.id} modelName={model.name} />;
}

/// Hooks live in this inner component so the early return above is safe:
/// when `model === null` we render nothing, period — same hook call order
/// (0 hooks) on every render. When `model` is non-null we mount the inner
/// component, which then unconditionally calls both queries.
function CardBody({ modelId, modelName }: { modelId: number; modelName: string }) {
  const docs = useModelDocuments(modelId);
  const checklists = useModelChecklists(modelId);
  const [actionError, setActionError] = useState<string | null>(null);
  const [viewerDoc, setViewerDoc] = useState<{ url: string; filename: string } | null>(null);

  const isLoading = docs.isLoading || checklists.isLoading;
  const error = docs.error || checklists.error;
  const docList: ModelDocument[] = docs.data ?? [];
  const checklistList: ChecklistSummary[] = checklists.data ?? [];

  const handleView = (doc: ModelDocument) => {
    setActionError(null);
    setViewerDoc({
      url: `/api/v1/aircraft-models/${modelId}/documents/${doc.id}/download`,
      filename: doc.original_filename,
    });
  };
  const handleDownload = async (doc: ModelDocument) => {
    setActionError(null);
    try {
      await downloadModelDocument(modelId, doc.id, doc.original_filename);
    } catch (e) {
      setActionError(`Download failed: ${(e as Error).message}`);
    }
  };

  return (
    <div className={styles.card}>
      <div className={styles.cardHeading}>Model documentation</div>

      {isLoading && <div className={styles.muted}>Loading…</div>}
      {error && (
        <div className={styles.error}>
          Could not load model documentation.
        </div>
      )}

      {!isLoading && !error && (
        <>
          {docList.length === 0 && checklistList.length === 0 ? (
            <div className={styles.emptyHint}>
              No documents or checklists for this model yet.{' '}
              <Link to={`/aircraft-models/${modelId}`} className={styles.footerLink}>
                Open model page →
              </Link>
            </div>
          ) : (
            <>
              {docList.length > 0 && (
                <div className={styles.subgroup}>
                  <div className={styles.subgroupHeading}>Manuals</div>
                  <div className={styles.rowList}>
                    {docList.map((doc) => (
                      <DocRow
                        key={doc.id}
                        doc={doc}
                        onView={() => handleView(doc)}
                        onDownload={() => handleDownload(doc)}
                      />
                    ))}
                  </div>
                </div>
              )}
              {checklistList.length > 0 && (
                <div className={styles.subgroup}>
                  <div className={styles.subgroupHeading}>Checklists</div>
                  <div className={styles.rowList}>
                    {checklistList.map((c) => (
                      <ChecklistRow key={c.id} checklist={c} />
                    ))}
                  </div>
                </div>
              )}
            </>
          )}
        </>
      )}

      {actionError && (
        <div className={styles.error} style={{ marginTop: 'var(--space-2)' }}>
          {actionError}
        </div>
      )}

      {!isLoading && !error && (docList.length > 0 || checklistList.length > 0) && (
        <div className={styles.footer}>
          <Link to={`/aircraft-models/${modelId}`} className={styles.footerLink}>
            View full {modelName} page →
          </Link>
        </div>
      )}

      {viewerDoc && (
        <PdfViewer
          url={viewerDoc.url}
          filename={viewerDoc.filename}
          onClose={() => setViewerDoc(null)}
        />
      )}
    </div>
  );
}

function DocRow({
  doc,
  onView,
  onDownload,
}: {
  doc: ModelDocument;
  onView: () => void;
  onDownload: () => void;
}) {
  // 'other' rows carry an operator-supplied title; manual rows don't (their
  // category label is the identifier). Mirror the same convention used on
  // DocumentsSection so the two surfaces stay consistent.
  const titleText = doc.category === 'other'
    ? (doc.title ?? doc.original_filename)
    : CATEGORY_LABEL[doc.category];

  return (
    <div className={styles.row}>
      <span className={`${styles.badge} ${CATEGORY_BADGE_CLASS[doc.category]}`}>
        {CATEGORY_BADGE_SHORT[doc.category]}
      </span>
      <div className={styles.rowMain}>
        <span className={styles.rowTitle}>{titleText}</span>
        <span className={styles.rowMeta}>
          {doc.original_filename} · uploaded {doc.uploaded_at.substring(0, 10)}
        </span>
      </div>
      <div className={styles.rowActions}>
        <button
          type="button"
          className={styles.iconButton}
          onClick={onView}
          aria-label={`View ${doc.original_filename}`}
          title="View"
        >
          <Eye size={14} />
        </button>
        <button
          type="button"
          className={styles.iconButton}
          onClick={onDownload}
          aria-label={`Download ${doc.original_filename}`}
          title="Download"
        >
          <Download size={14} />
        </button>
      </div>
    </div>
  );
}

function ChecklistRow({ checklist }: { checklist: ChecklistSummary }) {
  const isNormal = checklist.type === 'normal_ops';
  return (
    <div className={styles.row}>
      <span className={`${styles.badge} ${isNormal ? styles.badgeNormal : styles.badgeQrh}`}>
        {isNormal ? 'Normal Ops' : 'QRH'}
      </span>
      <div className={styles.rowMain}>
        <span className={styles.rowTitle}>{checklist.title}</span>
        <span className={styles.rowMeta}>
          Rev {checklist.revision}
          {checklist.revision_date ? ` · ${checklist.revision_date}` : ''}
          {' · '}
          {checklist.section_count} sec · {checklist.item_count} items · updated{' '}
          {checklist.updated_at.substring(0, 10)}
        </span>
      </div>
      <div className={styles.rowActions}>
        <Link
          to={`/checklists/${checklist.id}`}
          className={styles.iconButton}
          aria-label={`View checklist ${checklist.title}`}
          title="View checklist"
        >
          <Eye size={14} />
        </Link>
      </div>
    </div>
  );
}
