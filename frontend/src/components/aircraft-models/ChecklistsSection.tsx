// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (C) 2026 Lars Wallström

import { useState } from 'react';
import { useMutation, useQueryClient } from '@tanstack/react-query';
import { useNavigate } from 'react-router-dom';
import { Eye, Pencil, Plus, Trash2 } from 'lucide-react';
import { useModelChecklists } from '../../hooks/useFleetStats';
import { useCurrentUser } from '../../hooks/useCurrentUser';
import { userHasAnyRole } from '../../utils/auth';
import { deleteChecklist, type ChecklistSummary } from '../../api/checklists';
import { ChecklistStartModal, type ChecklistStartSource } from './ChecklistStartModal';
import styles from '../../pages/FleetStatsShared.module.css';

interface ChecklistsSectionProps {
  modelId: number;
  modelName: string;
}

export function ChecklistsSection({ modelId, modelName }: ChecklistsSectionProps) {
  const { data: currentUser } = useCurrentUser();
  const isAdmin = userHasAnyRole(currentUser, 'admin');
  const navigate = useNavigate();
  const queryClient = useQueryClient();

  const { data, isLoading, error } = useModelChecklists(modelId);
  const [showStart, setShowStart] = useState(false);
  const [deleteError, setDeleteError] = useState<string | null>(null);

  const deleteMutation = useMutation<void, Error, number>({
    mutationFn: (id) => deleteChecklist(id),
    onSuccess: () => {
      queryClient.invalidateQueries({ queryKey: ['model-checklists', modelId] });
      queryClient.invalidateQueries({ queryKey: ['all-org-checklists'] });
    },
    onError: (e) => setDeleteError(e.message),
  });

  const handleStart = (source: ChecklistStartSource) => {
    setShowStart(false);
    const base = `/aircraft-models/${modelId}/checklists/new`;
    if (source.kind === 'blank') {
      navigate(base);
    } else if (source.kind === 'default') {
      navigate(`${base}?defaults=1`);
    } else {
      navigate(`${base}?copy=${source.sourceChecklistId}`);
    }
  };

  return (
    <section className={styles.section}>
      <div
        style={{
          display: 'flex',
          alignItems: 'center',
          justifyContent: 'space-between',
          marginBottom: 'var(--space-3)',
        }}
      >
        <h2 className={styles.sectionTitle} style={{ marginBottom: 0 }}>
          Checklists
        </h2>
        {isAdmin && (
          <button
            type="button"
            onClick={() => setShowStart(true)}
            style={addButtonStyle}
            onMouseEnter={(e) => {
              e.currentTarget.style.filter = 'brightness(1.08)';
            }}
            onMouseLeave={(e) => {
              e.currentTarget.style.filter = '';
            }}
          >
            <Plus size={14} /> Add checklist
          </button>
        )}
      </div>

      {isLoading ? (
        <p style={{ color: 'var(--color-text-secondary)' }}>Loading checklists…</p>
      ) : error ? (
        <p style={{ color: 'var(--color-critical)' }}>
          Failed to load checklists: {(error as Error).message}
        </p>
      ) : (data ?? []).length === 0 ? (
        <div
          style={{
            padding: 'var(--space-4)',
            background: 'var(--color-bg-surface)',
            border: '1px dashed var(--color-border)',
            borderRadius: 'var(--radius-md)',
            textAlign: 'center',
            color: 'var(--color-text-muted)',
            fontSize: '0.88rem',
          }}
        >
          No checklists defined for this model.
        </div>
      ) : (
        <div
          style={{
            display: 'flex',
            flexDirection: 'column',
            gap: 'var(--space-2)',
          }}
        >
          {(data ?? []).map((c) => (
            <ChecklistRow
              key={c.id}
              checklist={c}
              modelId={modelId}
              isAdmin={isAdmin}
              onView={() => navigate(`/checklists/${c.id}`)}
              onEdit={() => navigate(`/aircraft-models/${modelId}/checklists/${c.id}/edit`)}
              onDelete={() => {
                if (
                  window.confirm(
                    `Delete checklist "${c.title}" (${c.type === 'normal_ops' ? 'Normal Ops' : 'QRH'})? This cannot be undone.`,
                  )
                ) {
                  setDeleteError(null);
                  deleteMutation.mutate(c.id);
                }
              }}
            />
          ))}
        </div>
      )}

      {deleteError && (
        <p
          style={{
            color: 'var(--color-critical)',
            fontSize: '0.85rem',
            marginTop: 'var(--space-2)',
          }}
        >
          {deleteError}
        </p>
      )}

      {showStart && (
        <ChecklistStartModal
          modelId={modelId}
          modelName={modelName}
          onClose={() => setShowStart(false)}
          onStart={handleStart}
        />
      )}
    </section>
  );
}

const addButtonStyle: React.CSSProperties = {
  display: 'inline-flex',
  alignItems: 'center',
  gap: 'var(--space-2)',
  background: 'var(--color-accent)',
  color: 'var(--color-text-inverse)',
  border: 'none',
  borderRadius: 'var(--radius-sm)',
  padding: 'var(--space-2) var(--space-3)',
  fontSize: '0.82rem',
  fontWeight: 500,
  cursor: 'pointer',
};

interface ChecklistRowProps {
  checklist: ChecklistSummary;
  modelId: number;
  isAdmin: boolean;
  onView: () => void;
  onEdit: () => void;
  onDelete: () => void;
}

function ChecklistRow({ checklist, isAdmin, onView, onEdit, onDelete }: ChecklistRowProps) {
  const isNormal = checklist.type === 'normal_ops';
  return (
    <div
      style={{
        display: 'flex',
        alignItems: 'center',
        gap: 'var(--space-3)',
        padding: 'var(--space-3) var(--space-4)',
        background: 'var(--color-bg-surface)',
        border: '1px solid var(--color-border)',
        borderRadius: 'var(--radius-md)',
      }}
    >
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

      <div style={{ flex: 1, minWidth: 0 }}>
        <div
          style={{
            fontFamily: 'var(--font-display)',
            fontSize: '0.95rem',
            color: 'var(--color-text-primary)',
            overflow: 'hidden',
            textOverflow: 'ellipsis',
            whiteSpace: 'nowrap',
          }}
        >
          {checklist.title}
        </div>
        <div
          style={{
            fontSize: '0.78rem',
            color: 'var(--color-text-muted)',
            marginTop: '2px',
          }}
        >
          Rev {checklist.revision}
          {checklist.revision_date ? ` · ${checklist.revision_date}` : ''}
          {' · '}
          {checklist.section_count} section{checklist.section_count === 1 ? '' : 's'}
          {' · '}
          {checklist.item_count} item{checklist.item_count === 1 ? '' : 's'}
          {' · '}
          updated {checklist.updated_at.substring(0, 10)}
        </div>
      </div>

      <div style={{ display: 'flex', gap: 'var(--space-2)', flexShrink: 0 }}>
        <button
          type="button"
          style={iconButtonStyle}
          onClick={onView}
          aria-label={`View ${checklist.title}`}
          title="View"
        >
          <Eye size={14} />
        </button>
        {isAdmin && (
          <>
            <button
              type="button"
              style={iconButtonStyle}
              onClick={onEdit}
              aria-label={`Edit ${checklist.title}`}
              title="Edit"
            >
              <Pencil size={14} />
            </button>
            <button
              type="button"
              style={iconButtonStyle}
              onClick={onDelete}
              aria-label={`Delete ${checklist.title}`}
              title="Delete"
            >
              <Trash2 size={14} />
            </button>
          </>
        )}
      </div>
    </div>
  );
}

const iconButtonStyle: React.CSSProperties = {
  background: 'transparent',
  border: '1px solid var(--color-border)',
  borderRadius: 'var(--radius-sm)',
  width: 30,
  height: 30,
  display: 'inline-flex',
  alignItems: 'center',
  justifyContent: 'center',
  color: 'var(--color-text-secondary)',
  cursor: 'pointer',
};
