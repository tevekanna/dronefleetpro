// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (C) 2026 Lars Wallström

import { useState } from 'react';
import { useQuery } from '@tanstack/react-query';
import { Link, useNavigate } from 'react-router-dom';
import { fetchFlights } from '../../api/flights';
import type { Flight } from '../../types/flight';
import styles from '../../pages/AircraftDetailPage.module.css';

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

function formatAltitude(m: number): string {
  if (!Number.isFinite(m) || m <= 0) return '—';
  return `${m.toFixed(0)} m`;
}

function formatDistance(m: number): string {
  if (!Number.isFinite(m) || m <= 0) return '—';
  if (m < 1000) return `${m.toFixed(0)} m`;
  return `${(m / 1000).toFixed(2)} km`;
}

function sourceLabel(source: string | null): string {
  if (!source) return '—';
  if (source === 'dji_import') return 'DJI Import';
  if (source === 'csv_import') return 'CSV Import';
  if (source === 'manual') return 'Manual';
  return source;
}

/**
 * Pilot cell. Renders pic_name as a Link to /pilots/:id when pilot_id is
 * set; falls back to a non-link span (or em dash) otherwise. Click is
 * stopped from propagating to the row so the navigation goes to the
 * pilot page, not the flight detail.
 */
function PilotCell({ flight }: { flight: Flight }) {
  if (flight.pilot_id && flight.pic_name) {
    return (
      <Link
        to={`/pilots/${flight.pilot_id}`}
        className={styles.pilotLink}
        onClick={e => e.stopPropagation()}
      >
        {flight.pic_name}
      </Link>
    );
  }
  if (flight.pic_name) return <>{flight.pic_name}</>;
  return <span className={styles.muted}>—</span>;
}

export function AircraftFlightLog({ aircraftId }: { aircraftId: number }) {
  const [page, setPage] = useState(1);
  const navigate = useNavigate();

  const { data, isLoading, error } = useQuery({
    queryKey: ['aircraft-flights', aircraftId, page],
    queryFn: () => fetchFlights({ aircraftId, page, perPage: PER_PAGE }),
  });

  return (
    <div className={styles.section}>
      <div className={styles.sectionHeading}>Flight log</div>
      {isLoading && <div className={styles.muted}>Loading flights…</div>}
      {error && (
        <div className={styles.errorBox}>
          Failed to load flights: {(error as Error).message}
        </div>
      )}
      {data && data.flights.length === 0 && (
        <div className={styles.muted} style={{ fontSize: '0.85rem' }}>
          No flights logged for this aircraft yet.
        </div>
      )}
      {data && data.flights.length > 0 && (
        <>
          <table className={styles.flightTable}>
            <thead>
              <tr>
                <th>Date</th>
                <th>Pilot</th>
                <th>Duration</th>
                <th>Max altitude</th>
                <th>Max distance</th>
                <th>Source</th>
              </tr>
            </thead>
            <tbody>
              {data.flights.map((f: Flight) => (
                <tr
                  key={f.id}
                  className={styles.flightRow}
                  onClick={() => navigate(`/flights/${f.id}`)}
                >
                  <td className={styles.mono}>
                    {formatDate(f.flight_start ?? f.flight_datetime)}
                  </td>
                  <td>
                    <PilotCell flight={f} />
                  </td>
                  <td className={styles.mono}>{formatDuration(f.duration_seconds)}</td>
                  <td className={styles.mono}>{formatAltitude(f.max_altitude_m)}</td>
                  <td className={styles.mono}>{formatDistance(f.max_distance_m)}</td>
                  <td>{sourceLabel(f.log_source)}</td>
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
