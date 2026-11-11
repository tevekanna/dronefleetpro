// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (C) 2026 Lars Wallström

import { useQuery } from '@tanstack/react-query';
import { fetchMaintenanceDetail } from '../api/maintenance';
import type { MaintenanceDetail } from '../api/maintenance';

export function useMaintenanceDetail(id: number | undefined) {
  return useQuery<MaintenanceDetail>({
    queryKey: ['maintenance', 'detail', id],
    queryFn: () => fetchMaintenanceDetail(id as number),
    enabled: id !== undefined,
  });
}
