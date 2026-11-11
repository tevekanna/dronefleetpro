// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (C) 2026 Lars Wallström

import { useState } from 'react';
import { useQuery, useMutation, useQueryClient } from '@tanstack/react-query';
import { Navigate } from 'react-router-dom';
import { UserPlus, KeyRound } from 'lucide-react';
import {
  fetchUsers,
  deactivateUser,
  reactivateUser,
  issueResetToken,
} from '../api/users';
import { fetchPilots } from '../api/pilots';
import { useCurrentUser } from '../hooks/useCurrentUser';
import { userHasAnyRole } from '../utils/auth';
import { UserFormModal } from '../components/users/UserFormModal';
import { DeactivateUserModal } from '../components/users/DeactivateUserModal';
import { TokenDisplayModal } from '../components/users/TokenDisplayModal';
import type { CreateUserResponse, User, UserRole } from '../types/user';
import styles from './UserManagementPage.module.css';

const ROLE_LABELS: Record<UserRole, string> = {
  admin: 'Admin',
  pilot: 'Pilot',
  technician: 'Technician',
  safety_officer: 'Safety Officer',
  viewer: 'Viewer',
};

const ROLE_CLASS: Record<UserRole, string> = {
  admin: styles.roleAdmin,
  pilot: styles.rolePilot,
  technician: styles.roleTechnician,
  safety_officer: styles.roleSafetyOfficer,
  viewer: styles.roleViewer,
};

// Display order mirrors the backend primary-role precedence
// (admin > technician > safety_officer > pilot > viewer).
const ROLE_ORDER: UserRole[] = ['admin', 'technician', 'safety_officer', 'pilot', 'viewer'];

function sortRolesByPrecedence(roles: UserRole[]): UserRole[] {
  return [...roles].sort((a, b) => ROLE_ORDER.indexOf(a) - ROLE_ORDER.indexOf(b));
}

interface TokenDialogState {
  userName: string;
  token: string;
  expiresAt: string;
  intent: 'invitation' | 'password_reset';
}

function formatLastLogin(iso: string | null): string {
  if (!iso) return 'Never';
  const d = new Date(iso);
  return d.toISOString().slice(0, 16).replace('T', ' ') + ' UTC';
}

export function UserManagementPage() {
  const { data: currentUser, isLoading: loadingUser } = useCurrentUser();
  const queryClient = useQueryClient();

  const [includeInactive, setIncludeInactive] = useState(false);
  const [showForm, setShowForm] = useState(false);
  const [editUser, setEditUser] = useState<User | undefined>(undefined);
  const [deactivateTarget, setDeactivateTarget] = useState<User | null>(null);
  const [tokenDialog, setTokenDialog] = useState<TokenDialogState | null>(null);
  const [actionError, setActionError] = useState('');

  const usersQuery = useQuery({
    queryKey: ['users', includeInactive],
    queryFn: () => fetchUsers(includeInactive),
  });
  const pilotsQuery = useQuery({ queryKey: ['pilots'], queryFn: fetchPilots });

  const deactivateMutation = useMutation({
    mutationFn: deactivateUser,
    onSuccess: () => {
      queryClient.invalidateQueries({ queryKey: ['users'] });
      setDeactivateTarget(null);
    },
    onError: (err) => setActionError((err as Error).message),
  });

  const reactivateMutation = useMutation({
    mutationFn: reactivateUser,
    onSuccess: () => queryClient.invalidateQueries({ queryKey: ['users'] }),
    onError: (err) => setActionError((err as Error).message),
  });

  const resetTokenMutation = useMutation({
    mutationFn: issueResetToken,
    onError: (err) => setActionError((err as Error).message),
  });

  if (loadingUser) return <p className={styles.loading}>Loading…</p>;
  if (!currentUser || !userHasAnyRole(currentUser, 'admin')) {
    return <Navigate to="/dashboard" replace />;
  }

  const pilotMap = new Map((pilotsQuery.data ?? []).map((p) => [p.id, p.name]));

  const handleAdd = () => {
    setEditUser(undefined);
    setActionError('');
    setShowForm(true);
  };

  const handleEdit = (u: User) => {
    setEditUser(u);
    setActionError('');
    setShowForm(true);
  };

  const handleCreated = (resp: CreateUserResponse) => {
    setTokenDialog({
      userName: resp.user.name,
      token: resp.invitation_token,
      expiresAt: resp.expires_at,
      intent: 'invitation',
    });
  };

  const handleIssueReset = async (u: User) => {
    setActionError('');
    try {
      const resp = await resetTokenMutation.mutateAsync(u.id);
      setTokenDialog({
        userName: u.name,
        token: resp.token,
        expiresAt: resp.expires_at,
        intent: 'password_reset',
      });
    } catch {
      // error already captured by onError
    }
  };

  return (
    <div className={styles.page}>
      <div className={styles.header}>
        <h1 className={styles.title}>User Management</h1>
        <button className={styles.btnAdd} onClick={handleAdd}>
          <UserPlus size={16} />
          Add User
        </button>
      </div>

      <div className={styles.toolbar}>
        <label className={styles.toggleLabel}>
          <input
            type="checkbox"
            checked={includeInactive}
            onChange={(e) => setIncludeInactive(e.target.checked)}
          />
          Show inactive users
        </label>
      </div>

      {actionError && <p className={styles.error}>{actionError}</p>}
      {usersQuery.isLoading && <p className={styles.loading}>Loading users…</p>}
      {usersQuery.error && (
        <p className={styles.error}>
          Failed to load users: {(usersQuery.error as Error).message}
        </p>
      )}

      {usersQuery.data && usersQuery.data.length === 0 && (
        <div className={styles.empty}>
          <p className={styles.emptyTitle}>No users found.</p>
          <p className={styles.emptyHint}>Add the first user to get started.</p>
        </div>
      )}

      {usersQuery.data && usersQuery.data.length > 0 && (
        <table className={styles.table}>
          <thead>
            <tr>
              <th className={styles.th}>Name</th>
              <th className={styles.th}>User Code</th>
              <th className={styles.th}>Email</th>
              <th className={styles.th}>Role</th>
              <th className={styles.th}>Status</th>
              <th className={styles.th}>Linked Pilot</th>
              <th className={styles.th}>Last Login</th>
              <th className={styles.th}>Actions</th>
            </tr>
          </thead>
          <tbody>
            {usersQuery.data.map((u) => (
              <tr
                key={u.id}
                className={u.is_active ? undefined : styles.inactiveRow}
              >
                <td className={styles.td}>{u.name}</td>
                <td className={`${styles.td} ${styles.mono}`}>{u.user_code}</td>
                <td className={styles.td}>{u.email}</td>
                <td className={styles.td}>
                  <span className={styles.roleBadgeGroup}>
                    {sortRolesByPrecedence(
                      u.roles && u.roles.length > 0 ? u.roles : [u.role],
                    ).map((r) => (
                      <span
                        key={r}
                        className={`${styles.roleBadge} ${ROLE_CLASS[r] ?? ''}`}
                      >
                        {ROLE_LABELS[r] ?? r}
                      </span>
                    ))}
                  </span>
                </td>
                <td className={styles.td}>
                  <span className={styles.statusDot}>
                    <span
                      className={`${styles.dot} ${
                        u.is_active ? styles.dotActive : styles.dotInactive
                      }`}
                    />
                    {u.is_active ? 'Active' : 'Inactive'}
                  </span>
                </td>
                <td className={styles.td}>
                  {u.pilot_name ?? (u.pilot_id ? (pilotMap.get(u.pilot_id) ?? '—') : '—')}
                </td>
                <td className={`${styles.td} ${styles.lastLogin}`}>
                  {formatLastLogin(u.last_login_at)}
                </td>
                <td className={styles.td}>
                  <button className={styles.btnEdit} onClick={() => handleEdit(u)}>
                    Edit
                  </button>
                  {u.is_active && (
                    <button
                      className={styles.btnReset}
                      onClick={() => handleIssueReset(u)}
                      disabled={resetTokenMutation.isPending}
                    >
                      <KeyRound size={12} /> Reset
                    </button>
                  )}
                  {u.is_active && u.id !== currentUser.id && (
                    <button
                      className={styles.btnDeactivate}
                      onClick={() => setDeactivateTarget(u)}
                    >
                      Deactivate
                    </button>
                  )}
                  {!u.is_active && (
                    <button
                      className={styles.btnReactivate}
                      onClick={() => reactivateMutation.mutate(u.id)}
                      disabled={reactivateMutation.isPending}
                    >
                      Reactivate
                    </button>
                  )}
                </td>
              </tr>
            ))}
          </tbody>
        </table>
      )}

      <UserFormModal
        isOpen={showForm}
        user={editUser}
        onClose={() => setShowForm(false)}
        onCreated={handleCreated}
      />

      {deactivateTarget && (
        <DeactivateUserModal
          isOpen={deactivateTarget !== null}
          user={deactivateTarget}
          onConfirm={() => deactivateMutation.mutate(deactivateTarget.id)}
          onCancel={() => setDeactivateTarget(null)}
          isSubmitting={deactivateMutation.isPending}
        />
      )}

      {tokenDialog && (
        <TokenDisplayModal
          isOpen={tokenDialog !== null}
          userName={tokenDialog.userName}
          token={tokenDialog.token}
          expiresAt={tokenDialog.expiresAt}
          intent={tokenDialog.intent}
          onClose={() => setTokenDialog(null)}
        />
      )}
    </div>
  );
}
