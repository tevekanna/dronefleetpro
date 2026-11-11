// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (C) 2026 Lars Wallström

import { Plus, Trash2, X } from 'lucide-react';
import { uid } from '../../utils/uid';

// Draft shapes for the section/item builder. Shared by CreateTemplateModal and
// EditChecklistModal so both edit checklists with identical UX.
export type ItemDraft = {
  id: string;
  item_text: string;
  is_required: boolean;
  reference_manual: string;
};
export type SectionDraft = { id: string; section_name: string; items: ItemDraft[] };

// Internal only — consumers seed with newSection(); newItem is not exported.
const newItem = (): ItemDraft => ({
  id: uid(),
  item_text: '',
  is_required: true,
  reference_manual: '',
});

export const newSection = (): SectionDraft => ({
  id: uid(),
  section_name: '',
  items: [newItem()],
});

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
const labelStyle: React.CSSProperties = {
  fontSize: '0.75rem',
  color: 'var(--color-text-secondary)',
  fontFamily: 'var(--font-label)',
  marginBottom: 'var(--space-1)',
  display: 'block',
};
const sectionBoxStyle: React.CSSProperties = {
  background: 'var(--color-bg-surface)',
  border: '1px solid var(--color-border)',
  borderRadius: 'var(--radius-md)',
  padding: 'var(--space-4)',
  marginBottom: 'var(--space-4)',
};
const itemRowStyle: React.CSSProperties = {
  borderBottom: '1px solid var(--color-border)',
  paddingBottom: 'var(--space-3)',
  marginBottom: 'var(--space-3)',
};
const addBtnStyle: React.CSSProperties = {
  display: 'inline-flex',
  alignItems: 'center',
  gap: 'var(--space-1)',
  background: 'transparent',
  color: 'var(--color-accent)',
  border: '1px solid var(--color-border)',
  borderRadius: 'var(--radius-sm)',
  padding: 'var(--space-1) var(--space-3)',
  fontSize: '0.8rem',
  cursor: 'pointer',
};
const iconBtnStyle = (disabled: boolean): React.CSSProperties => ({
  background: 'transparent',
  border: 'none',
  color: disabled ? 'var(--color-text-muted)' : 'var(--color-critical)',
  cursor: disabled ? 'default' : 'pointer',
  display: 'inline-flex',
  alignItems: 'center',
  padding: '2px',
  opacity: disabled ? 0.4 : 1,
});

interface Props {
  sections: SectionDraft[];
  onChange: (next: SectionDraft[]) => void;
}

/**
 * Controlled section/item builder for inspection checklists. Owns no state —
 * the parent holds `sections` and receives every change via `onChange`.
 */
export function ChecklistSectionsEditor({ sections, onChange }: Props) {
  const updateSection = (id: string, patch: Partial<SectionDraft>) =>
    onChange(sections.map((s) => (s.id === id ? { ...s, ...patch } : s)));

  const updateItem = (sectionId: string, itemId: string, patch: Partial<ItemDraft>) =>
    onChange(
      sections.map((s) =>
        s.id === sectionId
          ? { ...s, items: s.items.map((it) => (it.id === itemId ? { ...it, ...patch } : it)) }
          : s,
      ),
    );

  const addItem = (sectionId: string) =>
    onChange(
      sections.map((s) => (s.id === sectionId ? { ...s, items: [...s.items, newItem()] } : s)),
    );

  const removeItem = (sectionId: string, itemId: string) =>
    onChange(
      sections.map((s) =>
        s.id === sectionId ? { ...s, items: s.items.filter((it) => it.id !== itemId) } : s,
      ),
    );

  const addSection = () => onChange([...sections, newSection()]);

  const removeSection = (sectionId: string) => {
    const target = sections.find((s) => s.id === sectionId);
    const hasContent =
      !!target &&
      (target.section_name.trim() !== '' || target.items.some((it) => it.item_text.trim() !== ''));
    if (hasContent && !window.confirm('Remove this section and all its items?')) {
      return;
    }
    onChange(sections.filter((s) => s.id !== sectionId));
  };

  return (
    <>
      {sections.map((section, si) => (
        <div key={section.id} style={sectionBoxStyle}>
          <div
            style={{
              display: 'flex',
              alignItems: 'flex-end',
              gap: 'var(--space-2)',
              marginBottom: 'var(--space-3)',
            }}
          >
            <div style={{ flex: 1 }}>
              <label style={labelStyle}>Section {si + 1} name *</label>
              <input
                style={inputStyle}
                value={section.section_name}
                onChange={(e) => updateSection(section.id, { section_name: e.target.value })}
                placeholder="e.g. Battery"
              />
            </div>
            <button
              type="button"
              style={iconBtnStyle(sections.length === 1)}
              disabled={sections.length === 1}
              onClick={() => removeSection(section.id)}
              title="Remove section"
            >
              <Trash2 size={15} />
            </button>
          </div>

          {section.items.map((item, ii) => (
            <div key={item.id} style={itemRowStyle}>
              <div style={{ display: 'flex', gap: 'var(--space-2)', alignItems: 'flex-start' }}>
                <span
                  style={{
                    fontSize: '0.75rem',
                    color: 'var(--color-text-muted)',
                    minWidth: '20px',
                    paddingTop: '6px',
                  }}
                >
                  {ii + 1}.
                </span>
                <div style={{ flex: 1 }}>
                  <textarea
                    style={{ ...inputStyle, minHeight: '44px', resize: 'vertical' }}
                    rows={2}
                    value={item.item_text}
                    onChange={(e) => updateItem(section.id, item.id, { item_text: e.target.value })}
                    placeholder="Inspection step text *"
                  />
                  <div
                    style={{
                      display: 'flex',
                      gap: 'var(--space-3)',
                      alignItems: 'center',
                      marginTop: 'var(--space-2)',
                    }}
                  >
                    <label
                      style={{
                        display: 'inline-flex',
                        alignItems: 'center',
                        gap: 'var(--space-1)',
                        fontSize: '0.8rem',
                        color: 'var(--color-text-secondary)',
                      }}
                    >
                      <input
                        type="checkbox"
                        checked={item.is_required}
                        onChange={(e) =>
                          updateItem(section.id, item.id, { is_required: e.target.checked })
                        }
                      />
                      Required
                    </label>
                    <input
                      style={{ ...inputStyle, flex: 1 }}
                      value={item.reference_manual}
                      onChange={(e) =>
                        updateItem(section.id, item.id, { reference_manual: e.target.value })
                      }
                      placeholder="e.g. DJI M600 Manual §I.1"
                    />
                  </div>
                </div>
                <button
                  type="button"
                  style={iconBtnStyle(section.items.length === 1)}
                  disabled={section.items.length === 1}
                  onClick={() => removeItem(section.id, item.id)}
                  title="Remove item"
                >
                  <X size={15} />
                </button>
              </div>
            </div>
          ))}

          <button type="button" style={addBtnStyle} onClick={() => addItem(section.id)}>
            <Plus size={14} /> Add item
          </button>
        </div>
      ))}

      <button
        type="button"
        style={{ ...addBtnStyle, marginBottom: 'var(--space-4)' }}
        onClick={addSection}
      >
        <Plus size={14} /> Add section
      </button>
    </>
  );
}

/**
 * Validate the builder drafts, returning an error message or null. Shared by
 * both consumers so create and edit enforce the same rules.
 */
export function validateSections(sections: SectionDraft[]): string | null {
  if (sections.length === 0) return 'At least one section is required.';
  for (const s of sections) {
    if (s.section_name.trim() === '') return 'Every section needs a name.';
    if (s.items.length === 0) return `Section "${s.section_name.trim()}" must have at least one item.`;
    for (const it of s.items) {
      if (it.item_text.trim() === '') return `Every item in "${s.section_name.trim()}" needs text.`;
    }
  }
  return null;
}

/** Convert builder drafts to the API `sections` payload (trimmed, ordered). */
export function sectionsToPayload(sections: SectionDraft[]) {
  return sections.map((s, si) => ({
    section_name: s.section_name.trim(),
    section_order: si + 1,
    items: s.items.map((it, ii) => ({
      item_text: it.item_text.trim(),
      is_required: it.is_required,
      reference_manual: it.reference_manual.trim() || undefined,
      item_order: ii + 1,
    })),
  }));
}
