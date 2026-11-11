// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (C) 2026 Lars Wallström

import { useRef, useState } from 'react';
import { useQuery, useQueryClient } from '@tanstack/react-query';
import {
  fetchOrgBranding,
  uploadOrgLogo,
  deleteOrgLogo,
  type LogoVariant,
  type LogoMeta,
} from '../../api/organisation';
import styles from './BrandingSection.module.css';

const MAX_BYTES = 2 * 1024 * 1024;
const ACCEPTED = ['image/png', 'image/jpeg'];

function formatSize(bytes: number): string {
  if (bytes < 1024) return `${bytes} B`;
  if (bytes < 1024 * 1024) return `${(bytes / 1024).toFixed(0)} kB`;
  return `${(bytes / 1024 / 1024).toFixed(1)} MB`;
}

interface SlotProps {
  variant: LogoVariant;
  label: string;
  hint: string;
  meta: LogoMeta | null;
  onChanged: () => void;
}

function LogoSlot({ variant, label, hint, meta, onChanged }: SlotProps) {
  const inputRef = useRef<HTMLInputElement>(null);
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);

  const pick = () => {
    setError(null);
    inputRef.current?.click();
  };

  const onFile = async (e: React.ChangeEvent<HTMLInputElement>) => {
    const file = e.target.files?.[0];
    e.target.value = ''; // allow re-selecting the same file
    if (!file) return;
    if (!ACCEPTED.includes(file.type)) {
      setError('Please choose a PNG or JPEG image.');
      return;
    }
    if (file.size > MAX_BYTES) {
      setError(`Image is too large (${formatSize(file.size)}). Maximum is 2 MB.`);
      return;
    }
    setBusy(true);
    setError(null);
    try {
      await uploadOrgLogo(variant, file);
      onChanged();
    } catch (err) {
      setError((err as Error).message);
    } finally {
      setBusy(false);
    }
  };

  const remove = async () => {
    setBusy(true);
    setError(null);
    try {
      await deleteOrgLogo(variant);
      onChanged();
    } catch (err) {
      setError((err as Error).message);
    } finally {
      setBusy(false);
    }
  };

  return (
    <div className={styles.slot}>
      <div className={styles.slotHead}>
        <span className={styles.slotLabel}>{label}</span>
        <span className={styles.slotHint}>{hint}</span>
      </div>
      <div className={styles.slotBody}>
        <div
          className={`${styles.preview} ${
            variant === 'light' ? styles.previewLight : styles.previewDark
          }`}
        >
          {meta ? (
            <img src={meta.data_url} alt={`${label} preview`} className={styles.previewImg} />
          ) : (
            <span className={styles.previewEmpty}>No logo</span>
          )}
        </div>
        <div className={styles.slotMeta}>
          {meta && (
            <>
              <span className={styles.metaName}>{meta.original_filename ?? '—'}</span>
              <span className={styles.metaSize}>
                {formatSize(meta.size_bytes)} · {meta.mime_type}
              </span>
            </>
          )}
          <div className={styles.slotActions}>
            <button type="button" className={styles.btn} onClick={pick} disabled={busy}>
              {busy ? 'Working…' : meta ? 'Replace' : 'Upload'}
            </button>
            {meta && (
              <button type="button" className={styles.btnGhost} onClick={remove} disabled={busy}>
                Remove
              </button>
            )}
          </div>
          {error && <span className={styles.error}>{error}</span>}
        </div>
      </div>
      <input
        ref={inputRef}
        type="file"
        accept="image/png,image/jpeg"
        className={styles.hiddenInput}
        onChange={onFile}
      />
    </div>
  );
}

export function BrandingSection() {
  const queryClient = useQueryClient();
  const { data, isLoading } = useQuery({
    queryKey: ['orgBranding'],
    queryFn: fetchOrgBranding,
  });

  const invalidate = () => {
    queryClient.invalidateQueries({ queryKey: ['orgBranding'] });
  };

  return (
    <div className={styles.wrap}>
      <h3 className={styles.heading}>Logo</h3>
      <p className={styles.intro}>
        Shown at the top of the sidebar, above the DroneFleetPro mark. Upload a PNG or JPEG (max
        2 MB). The light-theme logo is used in light mode and the dark-theme logo in dark mode; if
        only one is set it is used for both.
      </p>
      {isLoading ? (
        <p className={styles.loading}>Loading…</p>
      ) : (
        <div className={styles.slots}>
          <LogoSlot
            variant="light"
            label="Light theme logo"
            hint="Shown on light backgrounds"
            meta={data?.light ?? null}
            onChanged={invalidate}
          />
          <LogoSlot
            variant="dark"
            label="Dark theme logo"
            hint="Shown on dark backgrounds"
            meta={data?.dark ?? null}
            onChanged={invalidate}
          />
        </div>
      )}
    </div>
  );
}
