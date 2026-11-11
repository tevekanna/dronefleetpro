// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (C) 2026 Lars Wallström

import { useState, useEffect } from 'react';
import { useQuery, useQueryClient } from '@tanstack/react-query';
import { createUser, updateUser } from '../../api/users';
import { fetchPilots } from '../../api/pilots';
import type { CreateUserResponse, User, UserRole } from '../../types/user';
import styles from './UserFormModal.module.css';

const ROLES: ReadonlyArray<{ value: UserRole; label: string }> = [
  { value: 'admin', label: 'Admin' },
  { value: 'pilot', label: 'Pilot' },
  { value: 'technician', label: 'Technician' },
  { value: 'safety_officer', label: 'Safety Officer' },
  { value: 'viewer', label: 'Viewer' },
];

interface UserFormModalProps {
  isOpen: boolean;
  user?: User;
  onClose: () => void;
  /** Called when an update succeeds. */
  onUpdated?: (u: User) => void;
  /** Called when a create succeeds; receives the invitation token bundle. */
  onCreated?: (resp: CreateUserResponse) => void;
}

export function UserFormModal({
  isOpen,
  user,
  onClose,
  onUpdated,
  onCreated,
}: UserFormModalProps) {
  const queryClient = useQueryClient();
  const isEditing = !!user;
  const { data: pilots } = useQuery({ queryKey: ['pilots'], queryFn: fetchPilots });

  const [name, setName] = useState('');
  const [email, setEmail] = useState('');
  const [userCode, setUserCode] = useState('');
  const [roles, setRoles] = useState<UserRole[]>(['pilot']);
  const [pilotId, setPilotId] = useState('');
  const [errors, setErrors] = useState<Record<string, string>>({});
  const [formError, setFormError] = useState('');
  const [submitting, setSubmitting] = useState(false);

  useEffect(() => {
    if (isOpen) {
      setName(user?.name ?? '');
      setEmail(user?.email ?? '');
      setUserCode(user?.user_code ?? '');
      setRoles(
        user?.roles && user.roles.length > 0
          ? user.roles
          : user?.role
            ? [user.role]
            : ['pilot'],
      );
      setPilotId(user?.pilot_id ? String(user.pilot_id) : '');
      setErrors({});
      setFormError('');
    }
  }, [isOpen, user]);

  if (!isOpen) return null;

  const hasPilotRole = roles.includes('pilot');

  const toggleRole = (value: UserRole) => {
    setRoles((prev) =>
      prev.includes(value) ? prev.filter((r) => r !== value) : [...prev, value],
    );
  };

  const validate = (): boolean => {
    const errs: Record<string, string> = {};
    if (!name.trim()) errs.name = 'Name is required';
    if (!email.trim() || !email.includes('@')) errs.email = 'Valid email is required';
    if (!/^[A-Z]{3}$/.test(userCode.trim().toUpperCase())) {
      errs.userCode = 'Must be exactly 3 uppercase letters';
    }
    if (roles.length === 0) errs.roles = 'Select at least one role';
    if (hasPilotRole && !pilotId) {
      errs.pilotId = 'Pilot users must be linked to a pilot record so flight filters know who they are';
    }
    setErrors(errs);
    return Object.keys(errs).length === 0;
  };

  const handleSubmit = async (e: React.FormEvent) => {
    e.preventDefault();
    if (!validate()) return;

    setSubmitting(true);
    setFormError('');
    try {
      if (isEditing && user) {
        const updated = await updateUser(user.id, {
          name: name.trim(),
          email: email.trim(),
          user_code: userCode.trim().toUpperCase(),
          roles,
          pilot_id: pilotId ? Number(pilotId) : null,
        });
        queryClient.invalidateQueries({ queryKey: ['users'] });
        onUpdated?.(updated);
        onClose();
      } else {
        const created = await createUser({
          name: name.trim(),
          email: email.trim(),
          user_code: userCode.trim().toUpperCase(),
          roles,
          pilot_id: pilotId ? Number(pilotId) : null,
        });
        queryClient.invalidateQueries({ queryKey: ['users'] });
        onCreated?.(created);
        onClose();
      }
    } catch (err) {
      setFormError(err instanceof Error ? err.message : 'Failed to save user');
    } finally {
      setSubmitting(false);
    }
  };

  const activePilots = (pilots ?? []).filter((p) => p.is_active);

  return (
    <div className={styles.overlay} onClick={onClose}>
      <div className={styles.modal} onClick={(e) => e.stopPropagation()}>
        <h2 className={styles.title}>{isEditing ? 'Edit User' : 'Add User'}</h2>
        <form onSubmit={handleSubmit}>
          <div className={styles.field}>
            <label className={styles.label}>Full Name *</label>
            <input
              className={styles.input}
              value={name}
              onChange={(e) => setName(e.target.value)}
              placeholder="e.g. Alex Example"
            />
            {errors.name && <span className={styles.error}>{errors.name}</span>}
          </div>

          <div className={styles.field}>
            <label className={styles.label}>Email *</label>
            <input
              className={styles.input}
              type="email"
              value={email}
              onChange={(e) => setEmail(e.target.value)}
              placeholder="e.g. alex@example.com"
            />
            {errors.email && <span className={styles.error}>{errors.email}</span>}
          </div>

          <div className={styles.field}>
            <label className={styles.label}>User Code *</label>
            <input
              className={`${styles.input} ${styles.inputMono}`}
              value={userCode}
              onChange={(e) =>
                setUserCode(e.target.value.toUpperCase().slice(0, 3))
              }
              maxLength={3}
              placeholder="e.g. LAR"
            />
            <span className={styles.helperText}>
              3-letter identifier used in maintenance records and audit logs (e.g. LAR, JKL).
            </span>
            {errors.userCode && <span className={styles.error}>{errors.userCode}</span>}
          </div>

          <div className={styles.field}>
            <label className={styles.label}>Roles *</label>
            <div className={styles.roleCheckboxGroup}>
              {ROLES.map((r) => (
                <label key={r.value} className={styles.roleCheckboxRow}>
                  <input
                    type="checkbox"
                    checked={roles.includes(r.value)}
                    onChange={() => toggleRole(r.value)}
                  />
                  {r.label}
                </label>
              ))}
            </div>
            {roles.length === 0 && (
              <span className={styles.helperText}>Select at least one role to continue.</span>
            )}
            {errors.roles && <span className={styles.error}>{errors.roles}</span>}
          </div>

          <div className={styles.field}>
            <label className={styles.label}>
              Link to Pilot Profile{hasPilotRole ? ' *' : ''}
            </label>
            <select
              className={styles.select}
              value={pilotId}
              onChange={(e) => setPilotId(e.target.value)}
            >
              <option value="">None</option>
              {activePilots.map((p) => (
                <option key={p.id} value={p.id}>
                  {p.name}
                </option>
              ))}
            </select>
            <span className={styles.helperText}>
              {hasPilotRole
                ? 'Required for pilot users — controls which flights and stats they can see.'
                : 'Links this login account to a pilot profile for flight logging.'}
            </span>
            {errors.pilotId && <span className={styles.error}>{errors.pilotId}</span>}
          </div>

          {!isEditing && (
            <div className={styles.field}>
              <span className={styles.helperText}>
                An invitation link will be generated after creation. Share it with the
                user via a secure channel — they'll set their own password from the link.
              </span>
            </div>
          )}

          {formError && <p className={styles.formError}>{formError}</p>}

          <div className={styles.actions}>
            <button type="button" className={styles.btnCancel} onClick={onClose}>
              Cancel
            </button>
            <button
              type="submit"
              className={styles.btnSubmit}
              disabled={submitting || roles.length === 0}
            >
              {submitting ? 'Saving…' : isEditing ? 'Save Changes' : 'Create User'}
            </button>
          </div>
        </form>
      </div>
    </div>
  );
}
