// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (C) 2026 Lars Wallström

import { ChevronLeft, ChevronRight } from 'lucide-react';
import styles from './Pagination.module.css';

interface PaginationProps {
  page: number;
  totalPages: number;
  perPage: number;
  total: number;
  onPageChange: (page: number) => void;
  onPerPageChange: (perPage: number) => void;
  itemLabel?: string;   // defaults to "flights" for backwards-compat
}

function buildPageRange(current: number, total: number): (number | 'ellipsis')[] {
  if (total <= 7) {
    return Array.from({ length: total }, (_, i) => i + 1);
  }

  const pages: (number | 'ellipsis')[] = [1];

  if (current > 3) {
    pages.push('ellipsis');
  }

  const start = Math.max(2, current - 1);
  const end = Math.min(total - 1, current + 1);

  for (let i = start; i <= end; i++) {
    pages.push(i);
  }

  if (current < total - 2) {
    pages.push('ellipsis');
  }

  pages.push(total);
  return pages;
}

export function Pagination({ page, totalPages, perPage, total, onPageChange, onPerPageChange, itemLabel = 'flights' }: PaginationProps) {
  if (total === 0) {
    return <div className={styles.empty}>No {itemLabel} found</div>;
  }

  const from = (page - 1) * perPage + 1;
  const to = Math.min(page * perPage, total);
  const pages = buildPageRange(page, totalPages);

  return (
    <div className={styles.pagination}>
      <span className={styles.info}>
        Showing {from}–{to} of {total} {itemLabel}
      </span>
      <div className={styles.pages}>
        <button
          className={styles.navBtn}
          disabled={page <= 1}
          onClick={() => onPageChange(page - 1)}
        >
          <ChevronLeft size={16} />
        </button>
        {pages.map((p, idx) =>
          p === 'ellipsis' ? (
            <span key={`e${idx}`} className={styles.ellipsis}>...</span>
          ) : (
            <button
              key={p}
              className={`${styles.pageBtn} ${p === page ? styles.pageBtnActive : ''}`}
              onClick={() => onPageChange(p)}
            >
              {p}
            </button>
          )
        )}
        <button
          className={styles.navBtn}
          disabled={page >= totalPages}
          onClick={() => onPageChange(page + 1)}
        >
          <ChevronRight size={16} />
        </button>
      </div>
      <div className={styles.perPage}>
        <span>Per page:</span>
        <select
          className={styles.perPageSelect}
          value={perPage}
          onChange={e => onPerPageChange(Number(e.target.value))}
        >
          <option value={25}>25</option>
          <option value={50}>50</option>
          <option value={100}>100</option>
          <option value={200}>200</option>
        </select>
      </div>
    </div>
  );
}
