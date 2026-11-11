// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (C) 2026 Lars Wallström

import { useState } from 'react';
import { AddFlightModal } from '../components/flights/AddFlightModal';
import { FlightFilterBar } from '../components/flights/FlightFilterBar';
import { FlightsTable } from '../components/flights/FlightsTable';
import { ImportFlightLogModal } from '../components/flights/ImportFlightLogModal';
import { Pagination } from '../components/shared/Pagination';
import { StatsBar } from '../components/flights/StatsBar';
import { useFlightListFilters } from '../hooks/useFlightListFilters';
import { useCurrentUser } from '../hooks/useCurrentUser';
import { userHasAnyRole } from '../utils/auth';
import styles from './FlightLogPage.module.css';

export function FlightLogPage() {
  const [showAddFlight, setShowAddFlight] = useState(false);
  const [showImport, setShowImport] = useState(false);

  const { data: currentUser } = useCurrentUser();
  const canCreate = userHasAnyRole(currentUser, 'admin', 'safety_officer', 'technician', 'pilot');

  const {
    filters,
    flights: flightsData,
    isLoading,
    error,
    hasActiveFilters,
    patchFilters,
    resetFilters,
    setPage,
    setPerPage,
  } = useFlightListFilters();

  const items = flightsData?.flights ?? [];
  const showEmptyFiltered = !isLoading && hasActiveFilters && items.length === 0;
  const showEmptyUnfiltered = !isLoading && !hasActiveFilters && items.length === 0;

  return (
    <div>
      <div className={styles.header}>
        <h1 className={styles.title}>Flight Log</h1>
        <div className={styles.headerActions}>
          {canCreate && (
            <>
              <button onClick={() => setShowImport(true)} className={styles.btnImport}>
                Import log
              </button>
              <button onClick={() => setShowAddFlight(true)} className={styles.btnAdd}>
                Add Flight
              </button>
            </>
          )}
        </div>
      </div>

      <StatsBar />

      <FlightFilterBar
        filters={filters}
        onChange={(next) => patchFilters(next)}
        onReset={resetFilters}
      />

      {isLoading && <p className={styles.loading}>Loading flights…</p>}
      {error ? (
        <p className={styles.errorMessage}>
          Failed to load flights: {error instanceof Error ? error.message : String(error)}
        </p>
      ) : null}

      {showEmptyFiltered && (
        <div className={styles.empty}>
          <p className={styles.emptyTitle}>No flights match these filters</p>
          <p className={styles.emptyHint}>
            Try widening the date range, picking a different aircraft or pilot,
            or clear the filters to see everything.
          </p>
          <button className={styles.btnClearFilters} onClick={resetFilters}>
            Clear filters
          </button>
        </div>
      )}

      {showEmptyUnfiltered && (
        <div className={styles.empty}>
          <p className={styles.emptyTitle}>No flights yet</p>
          <p className={styles.emptyHint}>
            Import a log from DJI Fly, AirData, or Litchi — or add a flight manually.
          </p>
        </div>
      )}

      {!isLoading && items.length > 0 && <FlightsTable flights={items} />}

      {flightsData && items.length > 0 && (
        <Pagination
          page={flightsData.page}
          totalPages={flightsData.total_pages}
          perPage={flightsData.per_page}
          total={flightsData.total}
          onPageChange={setPage}
          onPerPageChange={setPerPage}
        />
      )}

      {showAddFlight && <AddFlightModal onClose={() => setShowAddFlight(false)} />}
      {showImport && (
        <ImportFlightLogModal onClose={() => setShowImport(false)} />
      )}
    </div>
  );
}
