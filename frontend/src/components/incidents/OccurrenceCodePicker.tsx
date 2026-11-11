// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (C) 2026 Lars Wallström

import { useMemo } from 'react';
import { useQuery } from '@tanstack/react-query';
import { fetchOccurrenceTaxonomy } from '../../api/occurrenceTaxonomy';
import type { OccurrenceCategory } from '../../api/occurrenceTaxonomy';

// Inline styles to match the existing incident-component pattern (see the
// comment at the top of ReportIncidentModal.tsx).
const selectStyle: React.CSSProperties = { background: 'var(--color-bg-input)', border: '1px solid var(--color-border)', borderRadius: 'var(--radius-sm)', padding: 'var(--space-2) var(--space-3)', color: 'var(--color-text-primary)', fontSize: '0.85rem', fontFamily: 'var(--font-body)', width: '100%', maxWidth: '100%' };
const fullTextStyle: React.CSSProperties = { fontSize: '0.8rem', color: 'var(--color-text-muted)', background: 'var(--color-bg-base)', border: '1px solid var(--color-border)', borderRadius: 'var(--radius-sm)', padding: 'var(--space-2) var(--space-3)', marginTop: 'var(--space-2)', whiteSpace: 'pre-wrap' };
const sourceStyle: React.CSSProperties = { fontSize: '0.72rem', color: 'var(--color-text-muted)', marginTop: 'var(--space-1)', fontStyle: 'italic' };
const codeStyle: React.CSSProperties = { fontFamily: 'var(--font-display)', color: 'var(--color-text-primary)' };

interface Group {
  label: string;
  items: OccurrenceCategory[];
}

// A native select sizes its dropdown to the longest <option>, and option text
// cannot be wrapped or truncated with CSS — so labels are truncated in JS.
// The full untruncated text of the selection is echoed below the select.
// Budget chosen so the widest truncated option renders at ~457px in Inter
// 0.85rem — no wider than the select itself inside the 520px incident modal.
const OPTION_TEXT_BUDGET = 55;

/// Truncate at a word boundary within `budget` characters, with an ellipsis.
function truncate(text: string, budget: number): string {
  if (text.length <= budget) return text;
  const cut = text.lastIndexOf(' ', budget);
  return `${text.slice(0, cut > 0 ? cut : budget)}…`;
}

/// Grouping key: sections 1 and 2 group by subsection; 3–8 by section only.
function groupLabel(c: OccurrenceCategory): string {
  if (c.subsection && c.subsection_title) {
    return `Section ${c.section} — ${c.section_title} · ${c.subsection} ${c.subsection_title}`;
  }
  return `Section ${c.section} — ${c.section_title}`;
}

export interface OccurrenceCodePickerProps {
  /** Selected Annex VI code, or '' for no classification. */
  value: string;
  onChange: (code: string) => void;
  disabled?: boolean;
}

/// Annex VI classification picker: all 78 categories grouped by section
/// (and by subsection within sections 1 and 2), each option showing the
/// code and the full regulatory text. The full text of the current
/// selection is echoed below the select, with the taxonomy's source
/// citation. Includes an explicit "no classification" option — callers
/// translate '' to an explicit null so the backend tri-state clears.
export function OccurrenceCodePicker({ value, onChange, disabled }: OccurrenceCodePickerProps) {
  // HOOKS — all before any early return (Rules of Hooks).
  const { data: taxonomy, isLoading, error } = useQuery({
    queryKey: ['occurrence-taxonomy'],
    queryFn: fetchOccurrenceTaxonomy,
    staleTime: Infinity, // immutable regulatory text
  });

  const groups = useMemo<Group[]>(() => {
    const out: Group[] = [];
    for (const c of taxonomy?.categories ?? []) {
      const label = groupLabel(c);
      const last = out[out.length - 1];
      if (last && last.label === label) last.items.push(c);
      else out.push({ label, items: [c] });
    }
    return out;
  }, [taxonomy]);

  const selected = useMemo(
    () => (taxonomy?.categories ?? []).find(c => c.code === value) ?? null,
    [taxonomy, value],
  );

  if (isLoading) {
    return <p style={{ fontSize: '0.8rem', color: 'var(--color-text-secondary)' }}>Loading Annex VI taxonomy…</p>;
  }
  if (error || !taxonomy) {
    return <p style={{ fontSize: '0.8rem', color: 'var(--color-critical)' }}>Failed to load taxonomy: {(error as Error | undefined)?.message ?? 'unknown error'}</p>;
  }

  return (
    <div>
      <select
        style={selectStyle}
        value={value}
        onChange={e => onChange(e.target.value)}
        disabled={disabled}
        aria-label="Annex VI occurrence classification"
      >
        <option value="">— No classification (clear) —</option>
        {groups.map(g => (
          <optgroup key={g.label} label={truncate(g.label, OPTION_TEXT_BUDGET)}>
            {g.items.map(c => (
              <option key={c.code} value={c.code}>
                {c.code} — {truncate(c.text, OPTION_TEXT_BUDGET)}
              </option>
            ))}
          </optgroup>
        ))}
      </select>
      {selected && (
        <div style={fullTextStyle}>
          <span style={codeStyle}>{selected.code}</span> — {selected.text}
        </div>
      )}
      <div style={sourceStyle}>{taxonomy.source}</div>
    </div>
  );
}
