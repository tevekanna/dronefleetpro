// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (C) 2026 Lars Wallström

import { useState } from 'react';
import { useSearchParams, useNavigate } from 'react-router-dom';
import { redeemToken } from '../api/auth';
import styles from './RedeemTokenPage.module.css';

const MIN_LEN = 12;

export function RedeemTokenPage() {
  const [params] = useSearchParams();
  const navigate = useNavigate();
  const tokenFromUrl = params.get('token') ?? '';

  const [token, setToken] = useState(tokenFromUrl);
  const [password, setPassword] = useState('');
  const [confirm, setConfirm] = useState('');
  const [error, setError] = useState('');
  const [submitting, setSubmitting] = useState(false);

  const handleSubmit = async (e: React.FormEvent) => {
    e.preventDefault();
    setError('');

    if (!token.trim()) {
      setError('No token provided. Use the link from your invitation email.');
      return;
    }
    if (password.length < MIN_LEN) {
      setError(`Password must be at least ${MIN_LEN} characters.`);
      return;
    }
    if (password !== confirm) {
      setError('Passwords do not match.');
      return;
    }

    setSubmitting(true);
    try {
      await redeemToken({ token: token.trim(), new_password: password });
      navigate('/login?reset=ok', { replace: true });
    } catch (err) {
      // Backend returns a generic "invalid or expired token" message — surface
      // it as-is, no fingerprinting which condition failed.
      setError(err instanceof Error ? err.message : 'invalid or expired token');
    } finally {
      setSubmitting(false);
    }
  };

  return (
    <div className={styles.container}>
      <div className={styles.card}>
        <h1 className={styles.title}>DroneFleetPro</h1>
        <p className={styles.subtitle}>Set your password</p>
        <p className={styles.intro}>
          Your administrator issued an invitation or reset link. Choose a new password
          (minimum {MIN_LEN} characters) to activate your account.
        </p>
        <form onSubmit={handleSubmit} className={styles.form}>
          {!tokenFromUrl && (
            <div className={styles.field}>
              <label htmlFor="token">Redemption token</label>
              <input
                id="token"
                value={token}
                onChange={(e) => setToken(e.target.value)}
                placeholder="Paste token here"
                autoComplete="off"
              />
            </div>
          )}
          <div className={styles.field}>
            <label htmlFor="password">New password</label>
            <input
              id="password"
              type="password"
              value={password}
              onChange={(e) => setPassword(e.target.value)}
              autoComplete="new-password"
              required
            />
          </div>
          <div className={styles.field}>
            <label htmlFor="confirm">Confirm new password</label>
            <input
              id="confirm"
              type="password"
              value={confirm}
              onChange={(e) => setConfirm(e.target.value)}
              autoComplete="new-password"
              required
            />
          </div>

          {error && <p className={styles.error}>{error}</p>}

          <button type="submit" className={styles.submitBtn} disabled={submitting}>
            {submitting ? 'Setting password…' : 'Set password & sign in'}
          </button>
        </form>
      </div>
    </div>
  );
}
