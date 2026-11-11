// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (C) 2026 Lars Wallström

import { apiClient } from './client';
import type {
  User,
  CreateUserPayload,
  CreateUserResponse,
  UpdateUserPayload,
  ResetTokenResponse,
} from '../types/user';

export async function fetchUsers(includeInactive = false): Promise<User[]> {
  const qs = includeInactive ? '?include_inactive=true' : '';
  return apiClient.get(`/users${qs}`);
}

export async function createUser(data: CreateUserPayload): Promise<CreateUserResponse> {
  return apiClient.post('/users', data);
}

export async function updateUser(id: number, data: UpdateUserPayload): Promise<User> {
  return apiClient.put(`/users/${id}`, data);
}

export async function deactivateUser(id: number): Promise<void> {
  return apiClient.delete(`/users/${id}`);
}

export async function reactivateUser(id: number): Promise<User> {
  return apiClient.post(`/users/${id}/reactivate`, undefined);
}

export async function issueResetToken(id: number): Promise<ResetTokenResponse> {
  return apiClient.post(`/users/${id}/reset-token`, undefined);
}
