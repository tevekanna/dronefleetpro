// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (C) 2026 Lars Wallström

import { useQuery } from '@tanstack/react-query';
import { fetchPilotDetail, type PilotDetail } from '../api/pilotDetail';

export function usePilotDetail(pilotId: number | undefined) {
  return useQuery<PilotDetail>({
    queryKey: ['pilot-detail', pilotId],
    queryFn: () => fetchPilotDetail(pilotId as number),
    enabled: pilotId !== undefined,
  });
}
