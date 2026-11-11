// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (C) 2026 Lars Wallström

import { useState, useEffect } from 'react';
import { useQuery } from '@tanstack/react-query';
import { useNavigate } from 'react-router-dom';
import { fetchSettings, saveSetting } from '../api/settings';
import { DataPurgeSection } from '../components/settings/DataPurgeSection';
import { PasswordChangeSection } from '../components/settings/PasswordChangeSection';
import { BrandingSection } from '../components/settings/BrandingSection';
import { getRolesFromToken, hasAnyRole } from '../utils/auth';
import styles from './SettingsPage.module.css';

const ROLE_LABELS: Record<string, string> = {
  admin: 'Admin',
  pilot: 'Pilot',
  technician: 'Technician',
  safety_officer: 'Safety Officer',
  viewer: 'Viewer',
};

function getUserCodeFromToken(): string {
  const token = localStorage.getItem('access_token');
  if (!token) return '—';
  try {
    const payload = JSON.parse(atob(token.split('.')[1]));
    return (payload.user_code as string) ?? '—';
  } catch {
    return '—';
  }
}

export function SettingsPage() {
  const { data: settings, isLoading } = useQuery({
    queryKey: ['settings'],
    queryFn: fetchSettings,
  });
  const userCode = getUserCodeFromToken();
  const roles = getRolesFromToken();
  const isAdmin = hasAnyRole('admin');
  const rolesLabel =
    roles.length > 0 ? roles.map((r) => ROLE_LABELS[r] ?? r).join(', ') : '—';
  const navigate = useNavigate();

  const [orgName, setOrgName] = useState('');
  const [easaOpRef, setEasaOpRef] = useState('');
  const [country, setCountry] = useState('');
  const [orgSaved, setOrgSaved] = useState(false);

  const [djiApiKey, setDjiApiKey] = useState('');
  const [showDjiKey, setShowDjiKey] = useState(false);
  const [djiSaved, setDjiSaved] = useState(false);

  useEffect(() => {
    if (settings) {
      setOrgName(settings['org_name'] ?? '');
      setEasaOpRef(settings['easa_op_ref'] ?? '');
      setCountry(settings['country'] ?? '');
      setDjiApiKey(settings['dji_api_key'] ?? '');
    }
  }, [settings]);

  const handleSaveOrg = async () => {
    await Promise.all([
      saveSetting('org_name', orgName),
      saveSetting('easa_op_ref', easaOpRef),
      saveSetting('country', country),
    ]);
    setOrgSaved(true);
    setTimeout(() => setOrgSaved(false), 3000);
  };

  const handleSaveDji = async () => {
    await saveSetting('dji_api_key', djiApiKey);
    setDjiSaved(true);
    setTimeout(() => setDjiSaved(false), 3000);
  };

  if (isLoading) return <p className={styles.loading}>Loading settings…</p>;

  return (
    <div>
      <h1 className={styles.title}>Settings</h1>

      <div className={styles.section}>
        <h2 className={styles.sectionTitle}>Organisation</h2>
        <div className={styles.field}>
          <label className={styles.label}>Organisation Name</label>
          <input
            className={styles.input}
            value={orgName}
            onChange={(e) => setOrgName(e.target.value)}
            placeholder="e.g. Nordic UAS Operations AB"
          />
        </div>
        <div className={styles.field}>
          <label className={styles.label}>EASA Operator Reference</label>
          <input
            className={styles.input}
            value={easaOpRef}
            onChange={(e) => setEasaOpRef(e.target.value)}
            placeholder="e.g. SE.UAS.001"
          />
        </div>
        <div className={styles.field}>
          <label className={styles.label}>Country</label>
          <input
            className={styles.input}
            value={country}
            onChange={(e) => setCountry(e.target.value)}
            placeholder="e.g. SE"
          />
        </div>
        <div className={styles.saveRow}>
          <button onClick={handleSaveOrg} className={styles.btnSave}>
            Save Organisation Settings
          </button>
          {orgSaved && <span className={styles.savedFlash}>Settings saved</span>}
        </div>
        <BrandingSection />
      </div>

      {isAdmin && (
        <div className={styles.section}>
          <h2 className={styles.sectionTitle}>User Management</h2>
          <p className={styles.userManagementHint}>
            Manage user accounts, roles, and access for your organisation.
          </p>
          <button onClick={() => navigate('/users')} className={styles.btnSave}>
            Manage Users
          </button>
        </div>
      )}

      <div className={styles.section}>
        <h2 className={styles.sectionTitle}>DJI Integration</h2>
        <div className={styles.field}>
          <label className={styles.label}>DJI Developer API Key</label>
          <div className={styles.djiKeyRow}>
            <input
              className={`${styles.input} ${styles.djiKeyInput}`}
              type={showDjiKey ? 'text' : 'password'}
              value={djiApiKey}
              onChange={(e) => setDjiApiKey(e.target.value)}
              placeholder="Enter your DJI API key"
            />
            <button
              type="button"
              onClick={() => setShowDjiKey(!showDjiKey)}
              className={styles.btnReveal}
            >
              {showDjiKey ? 'Hide' : 'Show'}
            </button>
          </div>
          <p className={styles.djiKeyHint}>
            Required for importing encrypted DJI flight logs. Get your key at
            developer.dji.com
          </p>
        </div>
        <div className={styles.saveRow}>
          <button onClick={handleSaveDji} className={styles.btnSave}>
            Save DJI Settings
          </button>
          {djiSaved && <span className={styles.savedFlash}>Settings saved</span>}
        </div>
      </div>

      <div className={styles.section}>
        <h2 className={styles.sectionTitle}>Application</h2>
        <div className={styles.infoRow}>
          <span className={styles.infoLabel}>Version</span>
          <span className={styles.infoValue}>DroneFleetPro v{__APP_VERSION__}</span>
        </div>
        <div className={styles.infoRow}>
          <span className={styles.infoLabel}>Backend</span>
          <span className={styles.infoValueOk}>Connected</span>
        </div>
        <div className={styles.infoRow}>
          <span className={styles.infoLabel}>User Code</span>
          <span className={styles.infoValue}>{userCode}</span>
        </div>
        <div className={`${styles.infoRow} ${styles.infoRowLast}`}>
          <span className={styles.infoLabel}>{roles.length > 1 ? 'Roles' : 'Role'}</span>
          <span className={styles.infoValue}>{rolesLabel}</span>
        </div>
      </div>

      <PasswordChangeSection />

      {isAdmin && <DataPurgeSection />}
    </div>
  );
}
