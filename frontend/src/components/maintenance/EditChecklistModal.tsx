// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (C) 2026 Lars Wallström

import { useEffect, useState } from 'react';
import { useQuery, useQueryClient } from '@tanstack/react-query';
import {
  fetchInstructionTemplateDetail,
  updateInstructionTemplateItems,
  createInstructionTemplateRevision,
  isRequiresNewRevision,
} from '../../api/maintenanceInstructionTemplates';
import type {
  EditTemplateRequest,
  TemplateDetail,
} from '../../types/maintenanceInstructionTemplate';
import { showToast } from '../../store/toastStore';
import { uid } from '../../utils/uid';
import {
  ChecklistSectionsEditor,
  validateSections,
  sectionsToPayload,
  type SectionDraft,
} from './ChecklistSectionsEditor';

interface Props {
  templateId: number;
  onClose: () => void;
}

const overlayStyle: React.CSSProperties = {
  position: 'fixed',
  inset: 0,
  background: 'rgba(0,0,0,0.7)',
  display: 'flex',
  alignItems: 'center',
  justifyContent: 'center',
  zIndex: 1100,
};
const modalStyle: React.CSSProperties = {
  background: 'var(--color-bg-elevated)',
  border: '1px solid var(--color-border-strong)',
  borderRadius: 'var(--radius-lg)',
  padding: 'var(--space-6)',
  width: '100%',
  maxWidth: '680px',
  boxShadow: 'var(--shadow-modal)',
  maxHeight: '90vh',
  overflowY: 'auto',
};
const fieldStyle: React.CSSProperties = { marginBottom: 'var(--space-4)' };
const labelStyle: React.CSSProperties = {
  fontSize: '0.75rem',
  color: 'var(--color-text-secondary)',
  fontFamily: 'var(--font-label)',
  marginBottom: 'var(--space-1)',
  display: 'block',
};
const inputStyle: React.CSSProperties = {
  background: 'var(--color-bg-input)',
  border: '1px solid var(--color-border)',
  borderRadius: 'var(--radius-sm)',
  padding: 'var(--space-2) var(--space-3)',
  color: 'var(--color-text-primary)',
  fontSize: '0.9rem',
  fontFamily: 'var(--font-body)',
  width: '100%',
  boxSizing: 'border-box',
};
const cancelBtnStyle: React.CSSProperties = {
  background: 'transparent',
  color: 'var(--color-text-secondary)',
  border: '1px solid var(--color-border)',
  borderRadius: 'var(--radius-sm)',
  padding: 'var(--space-2) var(--space-4)',
  cursor: 'pointer',
  fontSize: '0.85rem',
};
const primaryBtnStyle = (disabled: boolean): React.CSSProperties => ({
  background: 'var(--color-accent)',
  color: 'var(--color-text-inverse)',
  border: 'none',
  borderRadius: 'var(--radius-sm)',
  padding: 'var(--space-2) var(--space-4)',
  cursor: disabled ? 'default' : 'pointer',
  fontSize: '0.85rem',
  fontWeight: 500,
  opacity: disabled ? 0.7 : 1,
});
const headingStyle: React.CSSProperties = {
  fontFamily: 'var(--font-display)',
  fontSize: '1.1rem',
  color: 'var(--color-text-primary)',
  marginBottom: 'var(--space-5)',
};

/**
 * Compute the next revision label for display. Mirrors the backend rule
 * (`next_revision`): pure numeric increments zero-padded to the original width;
 * otherwise bump a trailing "-<n>" counter or append "-2".
 */
function nextRevision(current: string): string {
  const t = current.trim();
  if (t.length > 0 && /^\d+$/.test(t)) {
    return String(parseInt(t, 10) + 1).padStart(t.length, '0');
  }
  const m = t.match(/^(.*)-(\d+)$/);
  if (m) return `${m[1]}-${parseInt(m[2], 10) + 1}`;
  return `${t}-2`;
}

function detailToDrafts(detail: TemplateDetail): SectionDraft[] {
  return detail.sections.map((s) => ({
    id: uid(),
    section_name: s.section_name,
    items: s.items.map((it) => ({
      id: uid(),
      item_text: it.item_text,
      is_required: it.is_required,
      reference_manual: it.reference_manual ?? '',
    })),
  }));
}

/**
 * Edit an inspection checklist. Saving tries an in-place item replacement; if
 * the backend refuses (recorded completions exist) the modal switches to a
 * confirmation that saves the edit as the next revision, re-pointing linked
 * schedules. All hooks run before the loading/early-return branches.
 */
export function EditChecklistModal({ templateId, onClose }: Props) {
  const queryClient = useQueryClient();
  const detailQuery = useQuery({
    queryKey: ['instruction-template-detail', templateId],
    queryFn: () => fetchInstructionTemplateDetail(templateId),
  });

  const [name, setName] = useState('');
  const [description, setDescription] = useState('');
  const [sections, setSections] = useState<SectionDraft[]>([]);
  const [revisionMode, setRevisionMode] = useState(false);
  const [error, setError] = useState('');
  const [saving, setSaving] = useState(false);

  const detail = detailQuery.data;

  // Seed the builder once the detail loads (and whenever a different template
  // is targeted). Guarded on detail so it runs exactly on load.
  useEffect(() => {
    if (!detail) return;
    setName(detail.name);
    setDescription(detail.description ?? '');
    setSections(detailToDrafts(detail));
  }, [detail]);

  function buildBody(): EditTemplateRequest {
    return {
      name: name.trim(),
      description: description.trim() || undefined,
      sections: sectionsToPayload(sections),
    };
  }

  function finish() {
    void queryClient.invalidateQueries({ queryKey: ['instruction-templates'] });
    void queryClient.invalidateQueries({ queryKey: ['instruction-template-detail'] });
    void queryClient.invalidateQueries({ queryKey: ['aircraft-maintenance-detail'] });
    onClose();
  }

  async function handleSaveInPlace() {
    setError('');
    if (name.trim() === '') {
      setError('Name is required.');
      return;
    }
    const secError = validateSections(sections);
    if (secError) {
      setError(secError);
      return;
    }
    setSaving(true);
    try {
      await updateInstructionTemplateItems(templateId, buildBody());
      showToast('Checklist updated.', 'success');
      finish();
    } catch (e) {
      if (isRequiresNewRevision(e)) {
        setRevisionMode(true);
      } else {
        setError((e as Error).message);
      }
    } finally {
      setSaving(false);
    }
  }

  async function handleSaveRevision() {
    setError('');
    setSaving(true);
    try {
      const resp = await createInstructionTemplateRevision(templateId, buildBody());
      const n = resp.repointed_schedules;
      showToast(
        `Saved as revision ${resp.template.revision}. ${n} schedule${n === 1 ? '' : 's'} moved to the new revision.`,
        'success',
      );
      finish();
    } catch (e) {
      setError((e as Error).message);
    } finally {
      setSaving(false);
    }
  }

  return (
    <div style={overlayStyle} onClick={onClose}>
      <div style={modalStyle} onClick={(e) => e.stopPropagation()}>
        {detailQuery.isLoading && (
          <p style={{ color: 'var(--color-text-muted)' }}>Loading checklist…</p>
        )}
        {detailQuery.error && (
          <p style={{ color: 'var(--color-critical)' }}>
            Failed to load checklist: {(detailQuery.error as Error).message}
          </p>
        )}

        {detail && revisionMode && (
          <>
            <h2 style={headingStyle}>Save as new revision?</h2>
            <p style={{ color: 'var(--color-text-secondary)', fontSize: '0.9rem', lineHeight: 1.5 }}>
              This checklist has recorded completions and cannot be changed in place.
              Save as revision <strong>{nextRevision(detail.revision)}</strong>? Schedules
              linked to it will be moved to the new revision, and the current revision is
              kept for the audit trail.
            </p>
            {error && (
              <p style={{ color: 'var(--color-critical)', fontSize: '0.8rem', marginTop: 'var(--space-3)' }}>
                {error}
              </p>
            )}
            <div
              style={{
                display: 'flex',
                gap: 'var(--space-3)',
                justifyContent: 'flex-end',
                marginTop: 'var(--space-5)',
              }}
            >
              <button
                type="button"
                style={cancelBtnStyle}
                onClick={() => setRevisionMode(false)}
                disabled={saving}
              >
                Back
              </button>
              <button
                type="button"
                style={primaryBtnStyle(saving)}
                onClick={handleSaveRevision}
                disabled={saving}
              >
                {saving ? 'Saving…' : `Save as revision ${nextRevision(detail.revision)}`}
              </button>
            </div>
          </>
        )}

        {detail && !revisionMode && (
          <>
            <h2 style={headingStyle}>Edit checklist · rev. {detail.revision}</h2>

            <div style={fieldStyle}>
              <label style={labelStyle}>Name *</label>
              <input
                style={inputStyle}
                value={name}
                onChange={(e) => setName(e.target.value)}
                placeholder="e.g. Custom Pre-flight Inspection"
              />
            </div>

            <div style={fieldStyle}>
              <label style={labelStyle}>Description (optional)</label>
              <textarea
                style={{ ...inputStyle, minHeight: '52px', resize: 'vertical' }}
                value={description}
                onChange={(e) => setDescription(e.target.value)}
                placeholder="Short description of when this inspection applies."
              />
            </div>

            <ChecklistSectionsEditor sections={sections} onChange={setSections} />

            {error && (
              <p style={{ color: 'var(--color-critical)', fontSize: '0.8rem', marginBottom: 'var(--space-3)' }}>
                {error}
              </p>
            )}

            <div style={{ display: 'flex', gap: 'var(--space-3)', justifyContent: 'flex-end' }}>
              <button type="button" onClick={onClose} style={cancelBtnStyle} disabled={saving}>
                Cancel
              </button>
              <button
                type="button"
                onClick={handleSaveInPlace}
                style={primaryBtnStyle(saving)}
                disabled={saving}
              >
                {saving ? 'Saving…' : 'Save changes'}
              </button>
            </div>
          </>
        )}
      </div>
    </div>
  );
}
