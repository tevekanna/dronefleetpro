// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (C) 2026 Lars Wallström

import { useState, useCallback } from 'react';
import { ChevronRight } from 'lucide-react';
import styles from './ModelAccordion.module.css';

export interface AccordionGroup {
  groupKey: string;
  label: string;
  count: number;
  statusSummary?: string;
  /** Optional rich content rendered between the count pill and the
   *  right-aligned spacer. Useful for status badges (e.g. "1 overdue").
   *  Click events bubble to the header toggle by default; stop
   *  propagation in the node itself if needed. */
  headerExtras?: React.ReactNode;
  children: React.ReactNode;
}

interface ModelAccordionProps {
  groups: AccordionGroup[];
  storageKey: string;
  defaultOpen?: boolean;
  retiredCounts?: Record<string, number>;
  retiredVisible?: Record<string, boolean>;
  onRetiredToggle?: (groupKey: string) => void;
  /** When true, all groups render expanded regardless of stored state.
   *  Used by Fleet search so matches are visible without clicking. The
   *  per-group toggle still writes to localStorage so original state
   *  restores when forceExpand goes back to false. */
  forceExpand?: boolean;
}

function readStoredState(storageKey: string, groupKey: string, defaultOpen: boolean): boolean {
  try {
    const val = localStorage.getItem(`${storageKey}:${groupKey}`);
    if (val === null) return defaultOpen;
    return val === 'true';
  } catch {
    return defaultOpen;
  }
}

function writeStoredState(storageKey: string, groupKey: string, open: boolean): void {
  try {
    localStorage.setItem(`${storageKey}:${groupKey}`, String(open));
  } catch {
    // Ignore — private browsing or quota exceeded
  }
}

export function ModelAccordion({ groups, storageKey, defaultOpen = false, retiredCounts, retiredVisible, onRetiredToggle, forceExpand = false }: ModelAccordionProps) {
  return (
    <div className={styles.accordion}>
      {groups.map(group => (
        <AccordionItem
          key={group.groupKey}
          group={group}
          storageKey={storageKey}
          defaultOpen={defaultOpen}
          retiredCount={retiredCounts?.[group.groupKey] ?? 0}
          isRetiredVisible={retiredVisible?.[group.groupKey] ?? false}
          onRetiredToggle={onRetiredToggle}
          forceExpand={forceExpand}
        />
      ))}
    </div>
  );
}

function AccordionItem({ group, storageKey, defaultOpen, retiredCount, isRetiredVisible, onRetiredToggle, forceExpand }: {
  group: AccordionGroup;
  storageKey: string;
  defaultOpen: boolean;
  retiredCount: number;
  isRetiredVisible: boolean;
  onRetiredToggle?: (groupKey: string) => void;
  forceExpand: boolean;
}) {
  const [isOpen, setIsOpen] = useState(() =>
    readStoredState(storageKey, group.groupKey, defaultOpen)
  );

  const effectiveOpen = forceExpand || isOpen;

  const toggle = useCallback(() => {
    if (forceExpand) return; // can't collapse while a search is active
    setIsOpen(prev => {
      const next = !prev;
      writeStoredState(storageKey, group.groupKey, next);
      return next;
    });
  }, [storageKey, group.groupKey, forceExpand]);

  const handleRetiredClick = useCallback((e: React.MouseEvent) => {
    e.stopPropagation();
    onRetiredToggle?.(group.groupKey);
  }, [onRetiredToggle, group.groupKey]);

  return (
    <div>
      <div className={styles.groupHeader} onClick={toggle}>
        <ChevronRight
          size={16}
          className={`${styles.chevron} ${effectiveOpen ? styles.chevronOpen : ''}`}
        />
        <span className={styles.groupLabel}>{group.label}</span>
        <span className={styles.countPill}>{group.count}</span>
        {group.headerExtras}
        <span className={styles.spacer} />
        {retiredCount > 0 && onRetiredToggle && (
          <span
            className={isRetiredVisible ? styles.retiredToggleActive : styles.retiredToggle}
            onClick={handleRetiredClick}
          >
            {isRetiredVisible ? '\u00b7 Hide retired' : `\u00b7 Show ${retiredCount} retired`}
          </span>
        )}
        {group.statusSummary && (
          <span className={styles.statusSummary}>{group.statusSummary}</span>
        )}
      </div>
      <div className={`${styles.groupContent} ${effectiveOpen ? styles.groupContentOpen : ''}`}>
        <div className={styles.groupContentInner}>
          {group.children}
        </div>
      </div>
    </div>
  );
}
