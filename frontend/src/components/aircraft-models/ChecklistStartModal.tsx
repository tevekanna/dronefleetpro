// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (C) 2026 Lars Wallström

import { useMemo, useState } from 'react';
import { ArrowLeft, ClipboardCopy, FilePlus2, ListChecks } from 'lucide-react';
import { useAllOrgChecklists } from '../../hooks/useFleetStats';
import type { ChecklistSummary } from '../../api/checklists';
import styles from '../maintenance/MaintenanceModal.module.css';

export type ChecklistStartSource =
  | { kind: 'blank' }
  | { kind: 'default' }
  | { kind: 'copy'; sourceChecklistId: number };

interface ChecklistStartModalProps {
  modelId: number;
  modelName: string;
  onClose: () => void;
  onStart: (source: ChecklistStartSource) => void;
}

/// Step 1: three big cards (Blank / Default / Copy-from-existing).
/// Step 2 (Copy only): list of every checklist in the org grouped by model
/// name, with a search filter. Selecting one fires onStart and the parent
/// unmounts the modal.
export function ChecklistStartModal({
  modelId,
  modelName,
  onClose,
  onStart,
}: ChecklistStartModalProps) {
  const [step, setStep] = useState<'choose' | 'copy'>('choose');

  return (
    <div className={styles.overlay} onClick={onClose}>
      <div
        className={styles.modal}
        onClick={(e) => e.stopPropagation()}
        style={{ maxWidth: 600 }}
      >
        <h2 className={styles.title}>Start a new checklist for {modelName}</h2>

        {step === 'choose' ? (
          <ChooseStep
            onStart={onStart}
            onCopyOpen={() => setStep('copy')}
            onCancel={onClose}
          />
        ) : (
          <CopyStep
            currentModelId={modelId}
            onBack={() => setStep('choose')}
            onPick={(id) => onStart({ kind: 'copy', sourceChecklistId: id })}
            onCancel={onClose}
          />
        )}
      </div>
    </div>
  );
}

// ---------------------------------------------------------------------------
// Step 1 — Choose
// ---------------------------------------------------------------------------

function ChooseStep({
  onStart,
  onCopyOpen,
  onCancel,
}: {
  onStart: (source: ChecklistStartSource) => void;
  onCopyOpen: () => void;
  onCancel: () => void;
}) {
  return (
    <>
      <p
        style={{
          color: 'var(--color-text-secondary)',
          fontSize: '0.85rem',
          marginBottom: 'var(--space-4)',
        }}
      >
        Pick a starting point. You can still add, remove, or reorder sections
        and items in the editor.
      </p>

      <div
        style={{
          display: 'flex',
          flexDirection: 'column',
          gap: 'var(--space-3)',
        }}
      >
        <StartCard
          icon={<FilePlus2 size={20} />}
          title="Blank checklist"
          description="Start with no sections. Build it from scratch."
          onClick={() => onStart({ kind: 'blank' })}
        />
        <StartCard
          icon={<ListChecks size={20} />}
          title="Default sections"
          description="Pre-populated with Preflight, Before Take Off, Before Landing, and After Landing. No items — fill them in yourself."
          onClick={() => onStart({ kind: 'default' })}
        />
        <StartCard
          icon={<ClipboardCopy size={20} />}
          title="Copy from existing checklist"
          description="Start from a checklist already defined for any aircraft model in your organisation."
          onClick={onCopyOpen}
        />
      </div>

      <div className={styles.actions} style={{ marginTop: 'var(--space-5)' }}>
        <button type="button" className={styles.btnCancel} onClick={onCancel}>
          Cancel
        </button>
      </div>
    </>
  );
}

function StartCard({
  icon,
  title,
  description,
  onClick,
}: {
  icon: React.ReactNode;
  title: string;
  description: string;
  onClick: () => void;
}) {
  return (
    <button
      type="button"
      onClick={onClick}
      style={{
        display: 'flex',
        alignItems: 'flex-start',
        gap: 'var(--space-3)',
        textAlign: 'left',
        padding: 'var(--space-4)',
        background: 'var(--color-bg-surface)',
        border: '1px solid var(--color-border)',
        borderRadius: 'var(--radius-md)',
        color: 'var(--color-text-primary)',
        cursor: 'pointer',
        font: 'inherit',
        width: '100%',
      }}
      onMouseEnter={(e) => {
        e.currentTarget.style.borderColor = 'var(--color-accent)';
      }}
      onMouseLeave={(e) => {
        e.currentTarget.style.borderColor = 'var(--color-border)';
      }}
    >
      <span
        style={{
          flexShrink: 0,
          color: 'var(--color-accent)',
          marginTop: '2px',
        }}
      >
        {icon}
      </span>
      <span>
        <span
          style={{
            display: 'block',
            fontFamily: 'var(--font-display)',
            fontSize: '0.95rem',
            color: 'var(--color-text-primary)',
            marginBottom: '4px',
          }}
        >
          {title}
        </span>
        <span
          style={{
            display: 'block',
            fontSize: '0.82rem',
            color: 'var(--color-text-secondary)',
            lineHeight: 1.4,
          }}
        >
          {description}
        </span>
      </span>
    </button>
  );
}

// ---------------------------------------------------------------------------
// Step 2 — Copy from existing
// ---------------------------------------------------------------------------

function CopyStep({
  currentModelId,
  onBack,
  onPick,
  onCancel,
}: {
  currentModelId: number;
  onBack: () => void;
  onPick: (sourceId: number) => void;
  onCancel: () => void;
}) {
  const { data, isLoading, error } = useAllOrgChecklists(true);
  const [filter, setFilter] = useState('');

  const filtered = useMemo(() => {
    const q = filter.trim().toLowerCase();
    return (data ?? []).filter((c) => {
      if (!q) return true;
      return (
        c.title.toLowerCase().includes(q) ||
        c.model_name.toLowerCase().includes(q) ||
        c.type.toLowerCase().includes(q)
      );
    });
  }, [data, filter]);

  const grouped = useMemo(() => {
    const buckets = new Map<string, Array<ChecklistSummary & { model_name: string }>>();
    for (const c of filtered) {
      const arr = buckets.get(c.model_name) ?? [];
      arr.push(c);
      buckets.set(c.model_name, arr);
    }
    return Array.from(buckets.entries())
      .map(([model_name, items]) => ({
        model_name,
        items: items.sort((a, b) => a.type.localeCompare(b.type) || a.title.localeCompare(b.title)),
      }))
      .sort((a, b) => a.model_name.localeCompare(b.model_name));
  }, [filtered]);

  return (
    <>
      <div
        style={{
          display: 'flex',
          alignItems: 'center',
          gap: 'var(--space-2)',
          marginBottom: 'var(--space-3)',
        }}
      >
        <button
          type="button"
          className={styles.btnCancel}
          onClick={onBack}
          style={{ padding: 'var(--space-1) var(--space-3)' }}
        >
          <ArrowLeft size={14} /> Back
        </button>
        <span style={{ fontSize: '0.85rem', color: 'var(--color-text-secondary)' }}>
          Pick a source checklist to copy
        </span>
      </div>

      <input
        className={styles.input}
        type="text"
        placeholder="Filter by title, model, or type…"
        value={filter}
        onChange={(e) => setFilter(e.target.value)}
        style={{ marginBottom: 'var(--space-3)' }}
        autoFocus
      />

      {isLoading && (
        <p style={{ color: 'var(--color-text-secondary)' }}>Loading…</p>
      )}
      {error && (
        <div className={styles.errorBanner}>
          Failed to load checklists: {(error as Error).message}
        </div>
      )}
      {!isLoading && !error && grouped.length === 0 && (
        <p style={{ color: 'var(--color-text-muted)', fontSize: '0.85rem' }}>
          No checklists found{filter ? ' matching this filter' : ' in this organisation'}.
        </p>
      )}

      <div
        style={{
          maxHeight: '40vh',
          overflowY: 'auto',
          display: 'flex',
          flexDirection: 'column',
          gap: 'var(--space-3)',
        }}
      >
        {grouped.map((g) => (
          <div key={g.model_name}>
            <div
              style={{
                fontFamily: 'var(--font-label)',
                fontSize: '0.72rem',
                color: 'var(--color-text-muted)',
                textTransform: 'uppercase',
                letterSpacing: '0.05em',
                marginBottom: 'var(--space-2)',
              }}
            >
              {g.model_name}
              {g.items[0].model_id === currentModelId && ' (this model)'}
            </div>
            <div
              style={{
                display: 'flex',
                flexDirection: 'column',
                gap: 'var(--space-1)',
              }}
            >
              {g.items.map((c) => (
                <button
                  key={c.id}
                  type="button"
                  onClick={() => onPick(c.id)}
                  style={{
                    display: 'flex',
                    alignItems: 'center',
                    gap: 'var(--space-3)',
                    padding: 'var(--space-2) var(--space-3)',
                    background: 'var(--color-bg-surface)',
                    border: '1px solid var(--color-border)',
                    borderRadius: 'var(--radius-sm)',
                    color: 'var(--color-text-primary)',
                    cursor: 'pointer',
                    font: 'inherit',
                    textAlign: 'left',
                  }}
                  onMouseEnter={(e) => {
                    e.currentTarget.style.borderColor = 'var(--color-accent)';
                  }}
                  onMouseLeave={(e) => {
                    e.currentTarget.style.borderColor = 'var(--color-border)';
                  }}
                >
                  <TypeBadge type={c.type} />
                  <span style={{ flex: 1, fontSize: '0.9rem' }}>{c.title}</span>
                  <span
                    style={{
                      fontSize: '0.78rem',
                      color: 'var(--color-text-muted)',
                      fontFamily: 'var(--font-display)',
                    }}
                  >
                    Rev {c.revision}
                  </span>
                  <span
                    style={{
                      fontSize: '0.78rem',
                      color: 'var(--color-text-muted)',
                    }}
                  >
                    {c.section_count} sec · {c.item_count} items
                  </span>
                </button>
              ))}
            </div>
          </div>
        ))}
      </div>

      <div className={styles.actions} style={{ marginTop: 'var(--space-4)' }}>
        <button type="button" className={styles.btnCancel} onClick={onCancel}>
          Cancel
        </button>
      </div>
    </>
  );
}

function TypeBadge({ type }: { type: 'normal_ops' | 'qrh' }) {
  const isNormal = type === 'normal_ops';
  return (
    <span
      style={{
        display: 'inline-block',
        padding: '2px 8px',
        borderRadius: 'var(--radius-sm)',
        fontFamily: 'var(--font-display)',
        fontSize: '0.7rem',
        letterSpacing: '0.04em',
        background: isNormal
          ? 'color-mix(in srgb, var(--color-accent) 18%, transparent)'
          : 'color-mix(in srgb, var(--color-warning) 18%, transparent)',
        color: isNormal ? 'var(--color-accent)' : 'var(--color-warning)',
        border: `1px solid ${
          isNormal
            ? 'color-mix(in srgb, var(--color-accent) 35%, transparent)'
            : 'color-mix(in srgb, var(--color-warning) 35%, transparent)'
        }`,
        textTransform: 'uppercase',
        flexShrink: 0,
      }}
    >
      {isNormal ? 'Normal Ops' : 'QRH'}
    </span>
  );
}
