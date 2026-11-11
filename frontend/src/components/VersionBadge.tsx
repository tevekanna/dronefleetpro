// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (C) 2026 Lars Wallström

import { useEffect, useLayoutEffect, useRef, useState } from 'react';
import { createPortal } from 'react-dom';
import { AlertTriangle } from 'lucide-react';
import { useAppVersion } from '../hooks/useAppVersion';
import styles from './VersionBadge.module.css';

function formatIsoTimestamp(value: string | null): string {
  if (!value) return '—';
  // Trim fractional seconds and normalize to ISO 8601 YYYY-MM-DDTHH:MM:SSZ
  const d = new Date(value);
  if (isNaN(d.getTime())) return value;
  return d.toISOString().replace(/\.\d+Z$/, 'Z');
}

interface PopoverPosition {
  top: number;
  right: number;
}

export function VersionBadge() {
  const {
    frontendVersion,
    frontendBuildTimestamp,
    backendVersion,
    gitCommit,
    backendBuildTimestamp,
    versionsMatch,
    isLoading,
    isError,
  } = useAppVersion();

  const [open, setOpen] = useState(false);
  const [position, setPosition] = useState<PopoverPosition | null>(null);
  const badgeRef = useRef<HTMLButtonElement>(null);
  const popoverRef = useRef<HTMLDivElement>(null);

  // Render the popover via Portal into document.body so it escapes the
  // top-bar's stacking context and the sidebar's sticky stacking context.
  // Position: fixed against the badge's bounding rect keeps it visually
  // anchored — recomputed on resize and any ancestor scroll.
  useLayoutEffect(() => {
    if (!open) {
      setPosition(null);
      return;
    }
    const reposition = () => {
      const rect = badgeRef.current?.getBoundingClientRect();
      if (!rect) return;
      setPosition({
        top: rect.bottom + 8,
        right: Math.max(0, window.innerWidth - rect.right),
      });
    };
    reposition();
    window.addEventListener('resize', reposition);
    window.addEventListener('scroll', reposition, true);
    return () => {
      window.removeEventListener('resize', reposition);
      window.removeEventListener('scroll', reposition, true);
    };
  }, [open]);

  useEffect(() => {
    if (!open) return;
    const handleClick = (e: MouseEvent) => {
      const target = e.target as Node;
      if (badgeRef.current?.contains(target)) return;
      if (popoverRef.current?.contains(target)) return;
      setOpen(false);
    };
    const handleKey = (e: KeyboardEvent) => {
      if (e.key === 'Escape') setOpen(false);
    };
    document.addEventListener('mousedown', handleClick);
    document.addEventListener('keydown', handleKey);
    return () => {
      document.removeEventListener('mousedown', handleClick);
      document.removeEventListener('keydown', handleKey);
    };
  }, [open]);

  const showMismatchWarning = !isLoading && !isError && !versionsMatch;

  const popoverNode =
    open && position ? (
      <div
        ref={popoverRef}
        className={styles.popover}
        role="dialog"
        aria-label="Build details"
        style={{ top: position.top, right: position.right }}
      >
        <div className={styles.popoverHeader}>Build Details</div>
        <div className={styles.row}>
          <span className={styles.label}>Frontend</span>
          <span className={styles.value}>v{frontendVersion}</span>
        </div>
        <div className={styles.row}>
          <span className={styles.label}>Backend</span>
          <span className={styles.value}>
            {isLoading ? (
              <span className={styles.valueMuted}>loading…</span>
            ) : backendVersion ? (
              `v${backendVersion}`
            ) : (
              <span className={styles.valueMuted}>unavailable</span>
            )}
          </span>
        </div>
        <div className={styles.row}>
          <span className={styles.label}>Git commit</span>
          <span className={styles.value}>
            {gitCommit ?? <span className={styles.valueMuted}>unknown</span>}
          </span>
        </div>
        <div className={styles.row}>
          <span className={styles.label}>Backend built</span>
          <span className={styles.value}>{formatIsoTimestamp(backendBuildTimestamp)}</span>
        </div>
        <div className={styles.row}>
          <span className={styles.label}>Frontend built</span>
          <span className={styles.value}>{formatIsoTimestamp(frontendBuildTimestamp)}</span>
        </div>
        {showMismatchWarning && (
          <div className={styles.mismatch}>
            <AlertTriangle size={14} />
            <span>
              Frontend and backend versions differ. A hard refresh may be required
              after a backend deploy.
            </span>
          </div>
        )}
      </div>
    ) : null;

  return (
    <div className={styles.wrapper}>
      <button
        ref={badgeRef}
        type="button"
        className={styles.badge}
        aria-expanded={open}
        aria-label="Show build details"
        onClick={() => setOpen((v) => !v)}
      >
        <span>v{frontendVersion}</span>
        {showMismatchWarning && (
          <span className={styles.warnIcon} title="Frontend and backend versions differ">
            <AlertTriangle size={12} />
          </span>
        )}
      </button>
      {popoverNode && createPortal(popoverNode, document.body)}
    </div>
  );
}
