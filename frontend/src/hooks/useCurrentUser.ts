// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (C) 2026 Lars Wallström

import { useQuery } from '@tanstack/react-query';
import { apiClient } from '../api/client';
import type { User } from '../types/user';

export function useCurrentUser() {
  return useQuery({
    queryKey: ['currentUser'],
    queryFn: () => apiClient.get<User>('/auth/me'),
    retry: false,
    staleTime: 5 * 60 * 1000,
  });
}
