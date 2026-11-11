// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (C) 2026 Lars Wallström

import { useQuery } from '@tanstack/react-query';
import { Link, useNavigate } from 'react-router-dom';
import { fetchControllers } from '../../api/controllers';
import { useFleetStatsOverview } from '../../hooks/useFleetStats';
import { formatDaysAgo } from '../../pages/fleetStatsFormat';
import type { Controller } from '../../types/controller';
import { EmptyState } from './EmptyState';
import styles from './fleetStatsDashboard.module.css';

function StatusPill({ status }: { status: Controller['status'] }) {
  const color =
    status === 'active'
      ? 'var(--color-ok)'
      : status === 'maintenance'
      ? 'var(--color-warning)'
      : 'var(--color-text-muted)';
  return (
    <span style={{ color, fontFamily: 'var(--font-display)', fontSize: '0.78rem' }}>
      {status}
    </span>
  );
}

export function ControllersTab() {
  const navigate = useNavigate();
  const { data: list, isLoading } = useQuery({
    queryKey: ['controllers', 'active'],
    queryFn: () => fetchControllers(false),
    staleTime: 2 * 60 * 1000,
  });
  // Overview surfaces last_used for the top 5 recently-flown controllers — we
  // merge that into the list as an enrichment, since the controllers list
  // endpoint itself doesn't carry flight-aggregate data.
  const { data: overview } = useFleetStatsOverview();

  const lastUsedById = new Map<number, string | null>();
  for (const c of overview?.recently_flown_controllers_top5 ?? []) {
    lastUsedById.set(c.controller_id, c.last_used);
  }

  return (
    <>
      <section className={styles.section}>
        <header className={styles.sectionHeader}>
          <h2 className={styles.sectionTitle}>Controllers</h2>
          <span className={styles.sectionMeta}>active equipment</span>
        </header>
        {isLoading && <p style={{ color: 'var(--color-text-secondary)' }}>Loading…</p>}
        {!isLoading && (!list || list.length === 0) && (
          <EmptyState message="No controllers in the fleet yet." />
        )}
        {list && list.length > 0 && (
          <table className={styles.widgetTable}>
            <thead className={styles.widgetTableHead}>
              <tr>
                <th>ID</th>
                <th>Model</th>
                <th>Status</th>
                <th className={styles.cellRight}>Last used</th>
              </tr>
            </thead>
            <tbody>
              {list.map((c) => {
                const lastUsed = lastUsedById.get(c.id) ?? null;
                return (
                  <tr
                    key={c.id}
                    className={styles.widgetTableRow}
                    onClick={() => navigate(`/fleet-stats/controllers/${c.id}`)}
                  >
                    <td className={styles.cellId}>{c.equipment_id ?? c.serial_number}</td>
                    <td>{c.model ?? <span className={styles.cellMuted}>—</span>}</td>
                    <td>
                      <StatusPill status={c.status} />
                    </td>
                    <td className={`${styles.cellRight} ${styles.cellMuted}`}>
                      {formatDaysAgo(lastUsed)}
                    </td>
                  </tr>
                );
              })}
            </tbody>
          </table>
        )}
      </section>

      {list && list.length > 0 && (
        <Link to="/fleet-stats" className={styles.linkCard} aria-label="controller drill-down hint">
          <span>Click any row to open the controller's full usage statistics.</span>
          <span className={styles.linkCardArrow}>→</span>
        </Link>
      )}
    </>
  );
}
