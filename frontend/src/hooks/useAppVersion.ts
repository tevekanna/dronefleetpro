// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (C) 2026 Lars Wallström

import { useQuery } from '@tanstack/react-query';
import { apiClient } from '../api/client';

export interface HealthResponse {
  status: string;
  version: string;
  git_commit: string;
  build_timestamp: string;
}

export interface AppVersionInfo {
  frontendVersion: string;
  frontendBuildTimestamp: string;
  backendVersion: string | null;
  gitCommit: string | null;
  backendBuildTimestamp: string | null;
  versionsMatch: boolean;
  isLoading: boolean;
  isError: boolean;
}

export function useAppVersion(): AppVersionInfo {
  const { data, isLoading, isError } = useQuery({
    queryKey: ['health'],
    queryFn: () => apiClient.get<HealthResponse>('/health'),
    staleTime: Infinity,
    retry: 1,
  });

  const frontendVersion = __APP_VERSION__;
  const frontendBuildTimestamp = __BUILD_TIMESTAMP__;
  const backendVersion = data?.version ?? null;

  return {
    frontendVersion,
    frontendBuildTimestamp,
    backendVersion,
    gitCommit: data?.git_commit ?? null,
    backendBuildTimestamp: data?.build_timestamp ?? null,
    versionsMatch: backendVersion === null ? true : backendVersion === frontendVersion,
    isLoading,
    isError,
  };
}
