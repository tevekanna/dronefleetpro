// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (C) 2026 Lars Wallström

import { useQuery } from '@tanstack/react-query';
import { fetchAircraftDetail, type AircraftDetail } from '../api/aircraftDetail';

export function useAircraftDetail(aircraftId: number | undefined) {
  return useQuery<AircraftDetail>({
    queryKey: ['aircraft-detail', aircraftId],
    queryFn: () => fetchAircraftDetail(aircraftId as number),
    enabled: aircraftId !== undefined,
  });
}
