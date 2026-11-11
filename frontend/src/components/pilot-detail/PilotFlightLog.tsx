// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (C) 2026 Lars Wallström

import { useState } from 'react';
import { useQuery } from '@tanstack/react-query';
import { useNavigate } from 'react-router-dom';
import { fetchFlights } from '../../api/flights';
import type { Flight } from '../../types/flight';
import styles from '../../pages/PilotDetailPage.module.css';

const PER_PAGE = 25;

function formatDate(d: string | null): string {
  if (!d) return '—';
  return d.substring(0, 10);
}

function formatDuration(seconds: number): string {
  if (seconds <= 0) return '—';
  const h = Math.floor(seconds / 3600);
  const m = Math.floor((seconds % 3600) / 60);
  return `${h}h ${m}m`;
}

export function PilotFlightLog({ pilotId }: { pilotId: number }) {
  const [page, setPage] = useState(1);
  const navigate = useNavigate();

  const { data, isLoading, error } = useQuery({
    queryKey: ['pilot-flights', pilotId, page],
    queryFn: () => fetchFlights({ pilotId, page, perPage: PER_PAGE }),
  });

  return (
    <div className={styles.section}>
      <div className={styles.sectionHeading}>Flight log</div>
      {isLoading && <div className={styles.muted}>Loading flights…</div>}
      {error && (
        <div className={styles.errorBox}>Failed to load flights: {(error as Error).message}</div>
      )}
      {data && data.flights.length === 0 && (
        <div className={styles.muted} style={{ fontSize: '0.85rem' }}>
          No flights logged for this pilot yet.
        </div>
      )}
      {data && data.flights.length > 0 && (
        <>
          <table className={styles.flightTable}>
            <thead>
              <tr>
                <th>Date</th>
                <th>Aircraft</th>
                <th>Operation</th>
                <th>EASA</th>
                <th>Mission</th>
                <th>Duration</th>
              </tr>
            </thead>
            <tbody>
              {data.flights.map((f: Flight) => (
                <tr
                  key={f.id}
                  className={styles.flightRow}
                  onClick={() => navigate(`/flights/${f.id}`)}
                >
                  <td className={styles.mono}>{formatDate(f.flight_start ?? f.flight_datetime)}</td>
                  {/* TODO(aircraft-display-pt2): pending backend DTO extension — Flight.aircraft_name lacks equipment_id on this code path */}
                  <td>
                    {f.aircraft_name ?? f.aircraft_serial ?? (
                      <span className={styles.muted}>—</span>
                    )}
                  </td>
                  <td>
                    {f.operation_name ?? <span className={styles.muted}>—</span>}
                  </td>
                  <td className={styles.mono}>
                    {f.easa_category ?? <span className={styles.muted}>—</span>}
                  </td>
                  <td>{f.mission_type ?? <span className={styles.muted}>—</span>}</td>
                  <td className={styles.mono}>{formatDuration(f.duration_seconds)}</td>
                </tr>
              ))}
            </tbody>
          </table>
          <div className={styles.pagination}>
            <span>
              Page {data.page} of {data.total_pages} · {data.total} flight
              {data.total === 1 ? '' : 's'} total
            </span>
            <div className={styles.paginationButtons}>
              <button
                className={styles.paginationBtn}
                disabled={data.page <= 1}
                onClick={() => setPage(p => Math.max(1, p - 1))}
              >
                Previous
              </button>
              <button
                className={styles.paginationBtn}
                disabled={data.page >= data.total_pages}
                onClick={() => setPage(p => p + 1)}
              >
                Next
              </button>
            </div>
          </div>
        </>
      )}
    </div>
  );
}
