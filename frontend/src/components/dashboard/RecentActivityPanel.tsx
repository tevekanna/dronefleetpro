// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (C) 2026 Lars Wallström

import { Link, useNavigate } from 'react-router-dom';
import type { RecentFlight } from '../../api/dashboard';
import { formatISODate } from '../../utils/date';
import styles from './dashboardLayout.module.css';

export interface RecentActivityPanelProps {
  flights: RecentFlight[];
}

function formatDuration(seconds: number): string {
  const h = Math.floor(seconds / 3600);
  const m = Math.floor((seconds % 3600) / 60);
  if (h > 0) return `${h}h ${m.toString().padStart(2, '0')}m`;
  return `${m}m`;
}

function SourceBadge({ source }: { source: string | null }) {
  if (!source) return <span className={styles.muted}>—</span>;
  const isDji = source === 'dji_import';
  const label = isDji ? 'DJI' : source === 'manual' ? 'Manual' : source;
  return (
    <span
      className={`${styles.sourceBadge} ${
        isDji ? styles.sourceBadgeDji : styles.sourceBadgeOther
      }`}
    >
      {label}
    </span>
  );
}

export function RecentActivityPanel({ flights }: RecentActivityPanelProps) {
  const navigate = useNavigate();

  return (
    <section className={styles.panel} aria-labelledby="recent-activity-title">
      <header className={styles.panelHeader}>
        <h2 id="recent-activity-title" className={styles.panelTitle}>
          Recent activity
        </h2>
        <Link
          to="/flights"
          className={styles.panelLink}
          onClick={(e) => {
            e.preventDefault();
            navigate('/flights');
          }}
        >
          View all flights →
        </Link>
      </header>

      {flights.length === 0 ? (
        <div className={styles.empty}>No flights recorded yet</div>
      ) : (
        <table className={styles.table}>
          <thead className={styles.tableHead}>
            <tr>
              <th>Date</th>
              <th>Aircraft</th>
              <th>Duration</th>
              <th>EASA</th>
              <th>Source</th>
            </tr>
          </thead>
          <tbody>
            {flights.map((f) => (
              <tr
                key={f.id}
                className={styles.tableRow}
                onClick={() => navigate(`/flights/${f.id}`)}
              >
                <td className={styles.cellMono}>
                  {formatISODate(f.flight_datetime) || (
                    <span className={styles.muted}>—</span>
                  )}
                </td>
                {/* TODO(aircraft-display-pt2): pending backend DTO extension —
                    RecentFlight lacks serial_number so we can't yet route this
                    through <AircraftCell> / formatAircraftCell without producing
                    a duplicate "model · model" secondary line. Keep the ad-hoc
                    rendering until the backend exposes the SN. */}
                <td>
                  <div className={styles.aircraftCell}>
                    {f.equipment_id ? (
                      <span className={styles.equipmentId}>{f.equipment_id}</span>
                    ) : null}
                    <span className={styles.aircraftModel}>
                      {f.aircraft_name ?? '—'}
                    </span>
                  </div>
                </td>
                <td className={styles.cellMono}>{formatDuration(f.duration_seconds)}</td>
                <td>{f.easa_category ?? <span className={styles.muted}>—</span>}</td>
                <td>
                  <SourceBadge source={f.log_source} />
                </td>
              </tr>
            ))}
          </tbody>
        </table>
      )}
    </section>
  );
}
