// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (C) 2026 Lars Wallström

export type UserRole = 'admin' | 'pilot' | 'technician' | 'safety_officer' | 'viewer';

export interface User {
  id: number;
  org_id: number;
  name: string;
  email: string;
  user_code: string;
  /** Primary role — denormalised mirror the backend keeps in sync with `roles`. */
  role: UserRole;
  /** Full RBAC role set (source of truth). All authorisation must use this. */
  roles: UserRole[];
  is_active: boolean;
  deactivated_at: string | null;
  pilot_id: number | null;
  pilot_name: string | null;
  last_login_at: string | null;
  created_at: string;
}

export interface CreateUserPayload {
  name: string;
  email: string;
  user_code: string;
  roles: UserRole[];
  pilot_id?: number | null;
}

export interface CreateUserResponse {
  user: User;
  invitation_token: string;
  expires_at: string;
}

export interface UpdateUserPayload {
  name?: string;
  email?: string;
  user_code?: string;
  roles?: UserRole[];
  pilot_id?: number | null;
}

export interface ResetTokenResponse {
  token: string;
  expires_at: string;
}

export interface ChangePasswordPayload {
  current_password: string;
  new_password: string;
}

export interface RedeemTokenPayload {
  token: string;
  new_password: string;
}

export interface LoginRequest {
  email: string;
  password: string;
}

export interface LoginResponse {
  access_token: string;
  token_type: string;
  user: {
    id: number;
    org_id: number;
    name: string;
    email: string;
    user_code: string;
    role: UserRole;
    roles: UserRole[];
  };
}
