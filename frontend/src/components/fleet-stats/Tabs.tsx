// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (C) 2026 Lars Wallström

import { useId } from 'react';
import styles from './fleetStatsDashboard.module.css';

export interface TabDef<T extends string> {
  id: T;
  label: string;
}

export interface TabsProps<T extends string> {
  tabs: TabDef<T>[];
  active: T;
  onChange: (id: T) => void;
  children: React.ReactNode;
}

/**
 * Minimal accessible tabs. No external dep — pure ARIA tablist/tabpanel.
 * Active state lives in the parent so it can be reflected in the URL.
 */
export function Tabs<T extends string>({ tabs, active, onChange, children }: TabsProps<T>) {
  const base = useId();
  return (
    <div>
      <div role="tablist" className={styles.tabList}>
        {tabs.map((tab) => {
          const isActive = tab.id === active;
          return (
            <button
              key={tab.id}
              role="tab"
              type="button"
              aria-selected={isActive}
              aria-controls={`${base}-panel-${tab.id}`}
              id={`${base}-tab-${tab.id}`}
              tabIndex={isActive ? 0 : -1}
              className={`${styles.tabButton} ${isActive ? styles.tabButtonActive : ''}`}
              onClick={() => onChange(tab.id)}
            >
              {tab.label}
            </button>
          );
        })}
      </div>
      <div
        role="tabpanel"
        id={`${base}-panel-${active}`}
        aria-labelledby={`${base}-tab-${active}`}
        className={styles.tabPanel}
      >
        {children}
      </div>
    </div>
  );
}
