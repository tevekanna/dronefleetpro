// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (C) 2026 Lars Wallström

import styles from './FleetSearchBar.module.css';

interface FleetSearchBarProps {
  value: string;
  onChange: (value: string) => void;
  matchCount?: number;
  totalCount?: number;
  placeholder?: string;
}

export function FleetSearchBar({
  value,
  onChange,
  matchCount,
  totalCount,
  placeholder = 'Search by serial number, equipment ID, model, or nickname...',
}: FleetSearchBarProps) {
  const hasQuery = value.trim().length > 0;
  return (
    <div className={styles.container}>
      <div className={styles.inputWrapper}>
        <input
          type="search"
          className={styles.input}
          placeholder={placeholder}
          value={value}
          onChange={(e) => onChange(e.target.value)}
          aria-label="Search fleet"
        />
        {hasQuery && (
          <button
            type="button"
            className={styles.clearButton}
            onClick={() => onChange('')}
            aria-label="Clear search"
          >
            ×
          </button>
        )}
      </div>
      {hasQuery && totalCount !== undefined && matchCount !== undefined && (
        <span className={styles.matchInfo}>
          {matchCount} of {totalCount} match
        </span>
      )}
    </div>
  );
}

/**
 * Wraps text with <mark> elements highlighting all case-insensitive
 * occurrences of `query`. Returns the original text when query is empty.
 */
export function highlightMatch(
  text: string | null | undefined,
  query: string,
): React.ReactNode {
  if (!text) return text ?? '';
  const q = query.trim();
  if (!q) return text;
  const lowerText = text.toLowerCase();
  const lowerQuery = q.toLowerCase();
  const parts: React.ReactNode[] = [];
  let cursor = 0;
  let idx = lowerText.indexOf(lowerQuery, cursor);
  while (idx !== -1) {
    if (idx > cursor) parts.push(text.slice(cursor, idx));
    parts.push(
      <mark key={`m-${idx}`} className={styles.highlight}>
        {text.slice(idx, idx + q.length)}
      </mark>,
    );
    cursor = idx + q.length;
    idx = lowerText.indexOf(lowerQuery, cursor);
  }
  if (cursor < text.length) parts.push(text.slice(cursor));
  return <>{parts}</>;
}
