// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (C) 2026 Lars Wallström

import styles from './AircraftWorkspace.module.css';

export type WorkspaceTab = 'overview' | 'maintenance' | 'flights' | 'model';

export interface TabDef {
  key: WorkspaceTab;
  label: string;
}

interface Props {
  tabs: TabDef[];
  active: WorkspaceTab;
  onSelect: (tab: WorkspaceTab) => void;
}

/**
 * Tab bar for the aircraft workspace. Tabs and their visibility are decided by
 * the page (which applies role gating); this component just renders whatever
 * `tabs` array it is handed.
 */
export function AircraftWorkspaceTabs({ tabs, active, onSelect }: Props) {
  return (
    <nav className={styles.tabBar} role="tablist" aria-label="Aircraft workspace">
      {tabs.map((tab) => (
        <button
          key={tab.key}
          type="button"
          role="tab"
          aria-selected={active === tab.key}
          className={`${styles.tab} ${active === tab.key ? styles.tabActive : ''}`}
          onClick={() => onSelect(tab.key)}
        >
          {tab.label}
        </button>
      ))}
    </nav>
  );
}
