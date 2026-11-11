// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (C) 2026 Lars Wallström

import { apiClient } from './client';
import type { ChangePasswordPayload, RedeemTokenPayload } from '../types/user';

export async function changePassword(data: ChangePasswordPayload): Promise<void> {
  return apiClient.post('/auth/change-password', data);
}

export async function redeemToken(data: RedeemTokenPayload): Promise<void> {
  return apiClient.post('/auth/redeem-token', data);
}
