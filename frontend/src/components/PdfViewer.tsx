// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (C) 2026 Lars Wallström

import { useEffect, useRef, useState } from 'react';
import { Download, Printer, X } from 'lucide-react';
import styles from './PdfViewer.module.css';

interface PdfViewerProps {
  url: string;
  filename: string;
  onClose: () => void;
}

/**
 * Full-screen modal that fetches a PDF with the user's JWT and displays it
 * inline in an iframe. Offers Download (saves the fetched blob via a
 * temporary <a download> element) and Print (triggers the iframe's
 * print dialog) in a toolbar above the frame.
 *
 * The same blob URL backs both the iframe and the download link, so no
 * extra network round-trip is needed when the user clicks Download.
 */
export function PdfViewer({ url, filename, onClose }: PdfViewerProps) {
  const [blobUrl, setBlobUrl] = useState<string | null>(null);
  const [loading, setLoading] = useState(true);
  const [error, setError] = useState<string | null>(null);
  const iframeRef = useRef<HTMLIFrameElement | null>(null);

  // Fetch with JWT on mount; clean up object URL on unmount.
  useEffect(() => {
    let cancelled = false;
    let created: string | null = null;

    const fetchPdf = async () => {
      setLoading(true);
      setError(null);
      try {
        const token = localStorage.getItem('access_token');
        const res = await fetch(url, {
          headers: token ? { Authorization: `Bearer ${token}` } : {},
        });
        if (!res.ok) {
          let message = `Request failed: ${res.status}`;
          try {
            const body = await res.json();
            if (body?.error) message = body.error;
          } catch {
            // Non-JSON body (e.g. the PDF path hit before an error) — keep default message.
          }
          throw new Error(message);
        }
        const blob = await res.blob();
        const objectUrl = URL.createObjectURL(blob);
        created = objectUrl;
        if (!cancelled) {
          setBlobUrl(objectUrl);
        } else {
          URL.revokeObjectURL(objectUrl);
        }
      } catch (e) {
        if (!cancelled) setError(e instanceof Error ? e.message : 'Failed to load PDF');
      } finally {
        if (!cancelled) setLoading(false);
      }
    };
    fetchPdf();

    return () => {
      cancelled = true;
      if (created) URL.revokeObjectURL(created);
    };
  }, [url]);

  // Close on Esc.
  useEffect(() => {
    const onKey = (e: KeyboardEvent) => { if (e.key === 'Escape') onClose(); };
    document.addEventListener('keydown', onKey);
    return () => document.removeEventListener('keydown', onKey);
  }, [onClose]);

  const handleDownload = () => {
    if (!blobUrl) return;
    const link = document.createElement('a');
    link.href = blobUrl;
    link.download = filename;
    document.body.appendChild(link);
    link.click();
    document.body.removeChild(link);
  };

  const handlePrint = () => {
    const frame = iframeRef.current;
    if (!frame?.contentWindow) return;
    try {
      frame.contentWindow.focus();
      frame.contentWindow.print();
    } catch {
      // Some browsers block programmatic print against cross-origin frames;
      // blob: URLs are same-origin so this should work in practice.
    }
  };

  return (
    <div className={styles.overlay} role="dialog" aria-modal="true" aria-label="PDF viewer">
      <div className={styles.container}>
        <div className={styles.toolbar}>
          <div className={styles.title}>{filename}</div>
          <div className={styles.buttons}>
            <button
              type="button"
              className={`${styles.btn} ${styles.btnPrimary}`}
              onClick={handleDownload}
              disabled={!blobUrl}
            >
              <Download size={14} /> Download
            </button>
            <button
              type="button"
              className={styles.btn}
              onClick={handlePrint}
              disabled={!blobUrl}
            >
              <Printer size={14} /> Print
            </button>
            <button
              type="button"
              className={`${styles.btn} ${styles.btnClose}`}
              onClick={onClose}
              aria-label="Close"
              title="Close (Esc)"
            >
              <X size={16} />
            </button>
          </div>
        </div>
        <div className={styles.body}>
          {loading && (
            <div className={styles.centered}>
              <div className={styles.spinner} />
              <div>Loading report…</div>
            </div>
          )}
          {error && !loading && (
            <div className={styles.centered}>
              <div className={styles.errorBox}>
                Could not load the report.
                <br />
                <span style={{ fontFamily: 'var(--font-display)', fontSize: '0.8rem' }}>{error}</span>
              </div>
            </div>
          )}
          {blobUrl && !error && (
            <iframe
              ref={iframeRef}
              className={styles.frame}
              src={blobUrl}
              title={filename}
            />
          )}
        </div>
      </div>
    </div>
  );
}
