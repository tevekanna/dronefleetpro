// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (C) 2026 Lars Wallström

import { SOURCE_CODE_URL } from '../sourceCode';
import styles from './SourceCodeLink.module.css';

/** Small link to the application's source code, as AGPL-3.0 section 13 requires. */
export function SourceCodeLink() {
  return (
    <a
      className={styles.link}
      href={SOURCE_CODE_URL}
      target="_blank"
      rel="noopener noreferrer"
    >
      Source code (AGPL-3.0)
    </a>
  );
}
