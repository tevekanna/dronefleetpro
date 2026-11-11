// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (C) 2026 Lars Wallström

import { useQuery, useMutation, useQueryClient } from '@tanstack/react-query';
import { fetchMaintenance, createMaintenance } from '../api/maintenance';
import type { Maintenance } from '../types/maintenance';

export function useMaintenance() {
  return useQuery({
    queryKey: ['maintenance'],
    queryFn: () => fetchMaintenance(),
  });
}

export function useCreateMaintenance() {
  const queryClient = useQueryClient();
  return useMutation({
    mutationFn: (data: Partial<Maintenance>) => createMaintenance(data),
    onSuccess: () => {
      queryClient.invalidateQueries({ queryKey: ['maintenance'] });
    },
  });
}
