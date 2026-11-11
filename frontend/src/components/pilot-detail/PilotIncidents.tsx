// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (C) 2026 Lars Wallström

import { useNavigate } from 'react-router-dom';
import type { PilotDetailIncident } from '../../api/pilotDetail';
import styles from '../../pages/PilotDetailPage.module.css';

const SEVERITY_CLASS: Record<string, string> = {
  critical: styles.severityCritical,
  high: styles.severityHigh,
  medium: styles.severityMedium,
  low: styles.severityLow,
};

function formatDate(iso: string): string {
  return iso.substring(0, 10);
}

export function PilotIncidents({
  incidents,
}: {
  incidents: PilotDetailIncident[];
}) {
  const navigate = useNavigate();
  if (incidents.length === 0) return null;

  return (
    <div className={styles.section}>
      <div className={styles.sectionHeading}>Recent incidents</div>
      <div className={styles.incidentList}>
        {incidents.map(i => (
          <div
            key={i.id}
            className={`${styles.incidentRow} ${styles.flightRow}`}
            onClick={() => navigate(`/incidents`)}
            title="Open incidents page"
          >
            <span className={styles.mono}>{formatDate(i.occurred_at)}</span>
            <span>{i.category}</span>
            <span className={SEVERITY_CLASS[i.severity.toLowerCase()] ?? ''}>
              {i.severity}
            </span>
            <span className={styles.muted} style={{ fontSize: '0.8rem' }}>
              {i.description_preview}
              {i.description_preview.length >= 100 ? '…' : ''}
            </span>
            <span className={styles.mono}>{i.status}</span>
          </div>
        ))}
      </div>
    </div>
  );
}
