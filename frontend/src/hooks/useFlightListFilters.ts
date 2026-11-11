// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (C) 2026 Lars Wallström

import { useCallback, useMemo } from 'react';
import { useSearchParams } from 'react-router-dom';
import { keepPreviousData, useQuery } from '@tanstack/react-query';
import { fetchFlights, fetchFlightFilterOptions } from '../api/flights';
import type { FlightFilters, FlightListResponse } from '../types/flight';

const DEFAULT_PER_PAGE = 50;

const FILTER_KEYS = [
  'date_from',
  'date_to',
  'aircraft_id',
  'pilot_id',
  'mission_type',
  'easa_category',
  'search',
  'page',
  'per_page',
] as const;

function parsePosInt(raw: string | null): number | undefined {
  if (!raw) return undefined;
  const n = Number(raw);
  return Number.isFinite(n) && n > 0 ? n : undefined;
}

function paramsFromURL(sp: URLSearchParams): FlightFilters {
  return {
    dateFrom: sp.get('date_from') || undefined,
    dateTo: sp.get('date_to') || undefined,
    aircraftId: parsePosInt(sp.get('aircraft_id')),
    pilotId: parsePosInt(sp.get('pilot_id')),
    missionType: sp.get('mission_type') || undefined,
    easaCategory: sp.get('easa_category') || undefined,
    search: sp.get('search') || undefined,
    page: parsePosInt(sp.get('page')) ?? 1,
    perPage: parsePosInt(sp.get('per_page')) ?? DEFAULT_PER_PAGE,
  };
}

function writeParamsToURL(sp: URLSearchParams, next: FlightFilters): URLSearchParams {
  const out = new URLSearchParams();
  if (next.dateFrom) out.set('date_from', next.dateFrom);
  if (next.dateTo) out.set('date_to', next.dateTo);
  if (next.aircraftId) out.set('aircraft_id', String(next.aircraftId));
  if (next.pilotId) out.set('pilot_id', String(next.pilotId));
  if (next.missionType) out.set('mission_type', next.missionType);
  if (next.easaCategory) out.set('easa_category', next.easaCategory);
  if (next.search) out.set('search', next.search);
  if (next.page !== undefined && next.page !== 1) out.set('page', String(next.page));
  if (next.perPage !== undefined && next.perPage !== DEFAULT_PER_PAGE) {
    out.set('per_page', String(next.perPage));
  }
  // Preserve any unknown params (extensibility — non-FILTER_KEYS query state).
  sp.forEach((value, key) => {
    if (!out.has(key) && !(FILTER_KEYS as readonly string[]).includes(key)) {
      out.set(key, value);
    }
  });
  return out;
}

export interface UseFlightListFiltersResult {
  filters: FlightFilters;
  flights: FlightListResponse | undefined;
  isLoading: boolean;
  isFetching: boolean;
  error: unknown;
  hasActiveFilters: boolean;
  setFilters: (next: FlightFilters) => void;
  patchFilters: (patch: Partial<FlightFilters>) => void;
  resetFilters: () => void;
  setPage: (page: number) => void;
  setPerPage: (per_page: number) => void;
}

export function useFlightListFilters(): UseFlightListFiltersResult {
  const [searchParams, setSearchParams] = useSearchParams();
  const filters = useMemo(() => paramsFromURL(searchParams), [searchParams]);

  const flightsQuery = useQuery({
    queryKey: ['flights', filters],
    queryFn: () => fetchFlights(filters),
    placeholderData: keepPreviousData,
  });

  const setFilters = useCallback(
    (next: FlightFilters) => {
      // Default push (not replace) so browser back/forward navigates filter
      // history. The FlightFilterBar's debounced search input gates rapid
      // changes — at worst one history entry per typing pause.
      setSearchParams(writeParamsToURL(searchParams, next));
    },
    [searchParams, setSearchParams],
  );

  const patchFilters = useCallback(
    (patch: Partial<FlightFilters>) => {
      // Read live URL state inside the callback so concurrent debounced patches
      // (search input + date inputs) don't race on stale captures. Any filter
      // change resets to page 1.
      const current = paramsFromURL(new URLSearchParams(window.location.search));
      setFilters({ ...current, ...patch, page: 1 });
    },
    [setFilters],
  );

  const resetFilters = useCallback(() => {
    setFilters({ page: 1, perPage: filters.perPage });
  }, [setFilters, filters.perPage]);

  const setPage = useCallback(
    (page: number) => setFilters({ ...filters, page }),
    [setFilters, filters],
  );

  const setPerPage = useCallback(
    (perPage: number) => setFilters({ ...filters, perPage, page: 1 }),
    [setFilters, filters],
  );

  const hasActiveFilters = Boolean(
    filters.dateFrom ||
      filters.dateTo ||
      filters.aircraftId ||
      filters.pilotId ||
      filters.missionType ||
      filters.easaCategory ||
      filters.search,
  );

  return {
    filters,
    flights: flightsQuery.data,
    isLoading: flightsQuery.isLoading,
    isFetching: flightsQuery.isFetching,
    error: flightsQuery.error,
    hasActiveFilters,
    setFilters,
    patchFilters,
    resetFilters,
    setPage,
    setPerPage,
  };
}

export function useFlightFilterOptions() {
  return useQuery({
    queryKey: ['flight-filter-options'],
    queryFn: fetchFlightFilterOptions,
    staleTime: 5 * 60 * 1000,
  });
}
