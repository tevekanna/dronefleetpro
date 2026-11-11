// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (C) 2026 Lars Wallström

import { useState } from 'react';
import { Copy, Check } from 'lucide-react';
import styles from './TokenDisplayModal.module.css';

interface TokenDisplayModalProps {
  isOpen: boolean;
  userName: string;
  token: string;
  expiresAt: string;
  /** Banner copy: distinguishes invitation vs reset flows. */
  intent: 'invitation' | 'password_reset';
  onClose: () => void;
}

function formatExpiry(iso: string): string {
  try {
    return new Date(iso).toUTCString();
  } catch {
    return iso;
  }
}

export function TokenDisplayModal({
  isOpen,
  userName,
  token,
  expiresAt,
  intent,
  onClose,
}: TokenDisplayModalProps) {
  const [copied, setCopied] = useState(false);
  if (!isOpen) return null;

  const url = `${window.location.origin}${import.meta.env.BASE_URL}redeem?token=${encodeURIComponent(token)}`;

  const handleCopy = async () => {
    try {
      await navigator.clipboard.writeText(url);
      setCopied(true);
      setTimeout(() => setCopied(false), 2500);
    } catch {
      setCopied(false);
    }
  };

  const heading =
    intent === 'invitation' ? 'Invitation link generated' : 'Password reset link generated';

  return (
    <div className={styles.overlay} onClick={onClose}>
      <div className={styles.modal} onClick={(e) => e.stopPropagation()}>
        <h2 className={styles.title}>{heading}</h2>
        <p className={styles.intro}>
          For <span className={styles.userName}>{userName}</span>. Share this link with
          the user via a secure channel. Once you close this dialog the link cannot be
          retrieved again — issue a new one if you lose it.
        </p>
        <label className={styles.label}>Redemption link</label>
        <div className={styles.urlBox}>
          <code className={styles.url}>{url}</code>
        </div>
        <div className={styles.copyRow}>
          <button type="button" className={styles.btnCopy} onClick={handleCopy}>
            {copied ? <Check size={14} /> : <Copy size={14} />}
            {copied ? 'Copied' : 'Copy link'}
          </button>
          <span className={styles.expiry}>Expires: {formatExpiry(expiresAt)}</span>
        </div>
        <p className={styles.warning}>
          The link can only be redeemed once. After redemption the recipient is logged
          out and must log in with the new password they chose.
        </p>
        <div className={styles.actions}>
          <button type="button" className={styles.btnDismiss} onClick={onClose}>
            I have copied the link
          </button>
        </div>
      </div>
    </div>
  );
}
