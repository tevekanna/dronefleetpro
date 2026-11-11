// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (C) 2026 Lars Wallström

import { Component, type ErrorInfo, type ReactNode } from 'react';
import styles from './RouteErrorBoundary.module.css';

interface Props { children: ReactNode; }
interface State { hasError: boolean; isChunkError: boolean; }

// Dynamic-import / stale-chunk failures (common right after a deploy when
// hashed chunk files change) get a tailored "reload to update" message.
const CHUNK_ERROR = /ChunkLoadError|Loading chunk|dynamically imported module|Importing a module script failed/i;

export class RouteErrorBoundary extends Component<Props, State> {
  state: State = { hasError: false, isChunkError: false };

  static getDerivedStateFromError(error: unknown): State {
    const msg = error instanceof Error ? `${error.name} ${error.message}` : String(error);
    return { hasError: true, isChunkError: CHUNK_ERROR.test(msg) };
  }

  componentDidCatch(error: unknown, info: ErrorInfo) {
    // Surface for diagnostics; no telemetry pipeline yet.
    console.error('Route error boundary caught:', error, info);
  }

  private handleReload = () => { window.location.reload(); };

  render() {
    if (!this.state.hasError) return this.props.children;
    return (
      <div className={styles.wrap} role="alert">
        <h2 className={styles.title}>
          {this.state.isChunkError ? 'A new version is available' : 'Something went wrong'}
        </h2>
        <p className={styles.message}>
          {this.state.isChunkError
            ? 'This page failed to load, likely because the app was updated. Reload to get the latest version.'
            : 'An unexpected error occurred while loading this page.'}
        </p>
        <button type="button" className={styles.button} onClick={this.handleReload}>
          Reload
        </button>
      </div>
    );
  }
}
