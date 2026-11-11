// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (C) 2026 Lars Wallström

import { useState } from 'react';
import { Navigate } from 'react-router-dom';
import { useCurrentUser } from '../hooks/useCurrentUser';
import { userHasAnyRole } from '../utils/auth';
import { ExportTab } from '../components/data/ExportTab';
import { ImportTab } from '../components/data/ImportTab';
import styles from './DataManagementPage.module.css';

type Tab = 'export' | 'import';

export function DataManagementPage() {
  const { data: user, isLoading } = useCurrentUser();
  const [tab, setTab] = useState<Tab>('export');

  if (isLoading) return <p className={styles.loading}>Loading...</p>;
  if (!userHasAnyRole(user, 'admin')) return <Navigate to="/" replace />;

  return (
    <div className={styles.page}>
      <header className={styles.header}>
        <h1 className={styles.title}>Data Management</h1>
        <p className={styles.subtitle}>
          Export your organisation's data as a JSON snapshot, or import a snapshot
          from another instance. Admin-only.
        </p>
      </header>
      <div className={styles.tabs} role="tablist">
        <button
          role="tab"
          aria-selected={tab === 'export'}
          className={`${styles.tabBtn} ${tab === 'export' ? styles.tabBtnActive : ''}`}
          onClick={() => setTab('export')}
        >
          Export
        </button>
        <button
          role="tab"
          aria-selected={tab === 'import'}
          className={`${styles.tabBtn} ${tab === 'import' ? styles.tabBtnActive : ''}`}
          onClick={() => setTab('import')}
        >
          Import
        </button>
      </div>
      <div className={styles.tabPanel}>
        {tab === 'export' ? <ExportTab /> : <ImportTab />}
      </div>
    </div>
  );
}
