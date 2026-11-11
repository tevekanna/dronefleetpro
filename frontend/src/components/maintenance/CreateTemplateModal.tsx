// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (C) 2026 Lars Wallström

import { useState } from 'react';
import { useMutation, useQuery } from '@tanstack/react-query';
import { fetchAircraftModels } from '../../api/aircraftModels';
import { createInstructionTemplate } from '../../api/maintenanceInstructionTemplates';
import type { CreateTemplateRequest } from '../../types/maintenanceInstructionTemplate';
import {
  ChecklistSectionsEditor,
  newSection,
  validateSections,
  sectionsToPayload,
  type SectionDraft,
} from './ChecklistSectionsEditor';

interface Props {
  onClose: () => void;
  onSuccess: (newTemplateId: number) => void;
  defaultModelId?: number | null;
}

const overlayStyle: React.CSSProperties = {
  position: 'fixed',
  inset: 0,
  background: 'rgba(0,0,0,0.7)',
  display: 'flex',
  alignItems: 'center',
  justifyContent: 'center',
  // Raised above the schedule modal's overlay (z-index 1000) so the create
  // modal stacks above it when opened inline from ScheduleFormModal.
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
const fieldStyle: React.CSSProperties = {
  marginBottom: 'var(--space-4)',
};
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
const submitBtnStyle = (disabled: boolean): React.CSSProperties => ({
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
export function CreateTemplateModal({ onClose, onSuccess, defaultModelId }: Props) {
  const [name, setName] = useState('');
  const [modelId, setModelId] = useState<string>(
    defaultModelId != null ? String(defaultModelId) : '',
  );
  const [description, setDescription] = useState('');
  const [revision, setRevision] = useState('01');
  const [sections, setSections] = useState<SectionDraft[]>([newSection()]);
  const [error, setError] = useState('');
  const [success, setSuccess] = useState('');

  const { data: models } = useQuery({
    queryKey: ['aircraft-models', 'all'],
    queryFn: () => fetchAircraftModels(),
  });

  const mutation = useMutation({
    mutationFn: (payload: CreateTemplateRequest) => createInstructionTemplate(payload),
    onSuccess: (resp) => {
      setError('');
      setSuccess(`Template created — ${resp.name}, ${resp.items} items`);
      setTimeout(() => onSuccess(resp.id), 800);
    },
    onError: (e: Error) => setError(e.message),
  });

  const handleSubmit = (e: React.FormEvent) => {
    e.preventDefault();
    setError('');

    if (name.trim() === '') {
      setError('Template name is required.');
      return;
    }
    const sectionError = validateSections(sections);
    if (sectionError) {
      setError(sectionError);
      return;
    }

    const payload: CreateTemplateRequest = {
      name: name.trim(),
      aircraft_model_id: modelId ? Number(modelId) : null,
      description: description.trim() || undefined,
      revision: revision.trim() || undefined,
      sections: sectionsToPayload(sections),
    };

    mutation.mutate(payload);
  };

  const submitting = mutation.isPending || success !== '';

  return (
    <div style={overlayStyle} onClick={onClose}>
      <div style={modalStyle} onClick={(e) => e.stopPropagation()}>
        <h2
          style={{
            fontFamily: 'var(--font-display)',
            fontSize: '1.1rem',
            color: 'var(--color-text-primary)',
            marginBottom: 'var(--space-5)',
          }}
        >
          New Inspection Template
        </h2>

        <form onSubmit={handleSubmit}>
          <div style={fieldStyle}>
            <label style={labelStyle}>Name *</label>
            <input
              style={inputStyle}
              value={name}
              onChange={(e) => setName(e.target.value)}
              placeholder="e.g. Custom Pre-flight Inspection"
              required
            />
          </div>

          <div style={{ display: 'flex', gap: 'var(--space-4)' }}>
            <div style={{ ...fieldStyle, flex: 1 }}>
              <label style={labelStyle}>Aircraft model (optional)</label>
              <select
                style={inputStyle}
                value={modelId}
                onChange={(e) => setModelId(e.target.value)}
              >
                <option value="">— Generic (no model) —</option>
                {(models ?? []).map((m) => (
                  <option key={m.id} value={m.id}>
                    {m.name}
                  </option>
                ))}
              </select>
            </div>
            <div style={{ ...fieldStyle, width: '120px' }}>
              <label style={labelStyle}>Revision</label>
              <input
                style={inputStyle}
                value={revision}
                onChange={(e) => setRevision(e.target.value.slice(0, 10))}
                maxLength={10}
                placeholder="01"
              />
            </div>
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
          {success && (
            <p style={{ color: 'var(--color-ok)', fontSize: '0.8rem', marginBottom: 'var(--space-3)' }}>
              {success}
            </p>
          )}

          <div style={{ display: 'flex', gap: 'var(--space-3)', justifyContent: 'flex-end' }}>
            <button type="button" onClick={onClose} style={cancelBtnStyle}>
              Cancel
            </button>
            <button type="submit" disabled={submitting} style={submitBtnStyle(submitting)}>
              {mutation.isPending ? 'Creating…' : 'Create Template'}
            </button>
          </div>
        </form>
      </div>
    </div>
  );
}
