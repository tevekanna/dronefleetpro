// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (C) 2026 Lars Wallström

import { apiClient } from './client';
import type {
  Flight,
  CreateFlightRequest,
  FlightStats,
  FlightFilters,
  FlightFilterOptions,
  FlightListResponse,
} from '../types/flight';

export type { FlightFilters };

export async function fetchFlights(filters: FlightFilters = {}): Promise<FlightListResponse> {
  const params = new URLSearchParams();
  if (filters.dateFrom) params.set('date_from', filters.dateFrom);
  if (filters.dateTo) params.set('date_to', filters.dateTo);
  if (filters.aircraftId) params.set('aircraft_id', String(filters.aircraftId));
  if (filters.pilotId) params.set('pilot_id', String(filters.pilotId));
  if (filters.missionType) params.set('mission_type', filters.missionType);
  if (filters.easaCategory) params.set('easa_category', filters.easaCategory);
  if (filters.search) params.set('search', filters.search);
  if (filters.page) params.set('page', String(filters.page));
  if (filters.perPage) params.set('per_page', String(filters.perPage));
  const query = params.toString() ? `?${params}` : '';
  return apiClient.get(`/flights${query}`);
}

export async function fetchFlightById(id: number): Promise<Flight> {
  return apiClient.get(`/flights/${id}`);
}

export async function createFlight(data: CreateFlightRequest): Promise<Flight> {
  return apiClient.post('/flights', data);
}

export async function updateFlight(id: number, data: Partial<CreateFlightRequest>): Promise<Flight> {
  return apiClient.put(`/flights/${id}`, data);
}

export async function deleteFlight(id: number): Promise<void> {
  return apiClient.delete(`/flights/${id}`);
}

export async function fetchFlightStats(): Promise<FlightStats> {
  return apiClient.get('/flights/stats');
}

export async function fetchFlightFilterOptions(): Promise<FlightFilterOptions> {
  return apiClient.get('/flights/filters');
}
