// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (C) 2026 Lars Wallström

import { useQuery } from '@tanstack/react-query';
import { fetchFlightDetail, type FlightDetail } from '../api/flightDetail';

export function useFlightDetail(flightId: number | undefined) {
  return useQuery<FlightDetail>({
    queryKey: ['flight-detail', flightId],
    queryFn: () => fetchFlightDetail(flightId as number),
    enabled: flightId !== undefined,
  });
}
