// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (C) 2026 Lars Wallström

import { useNavigate, useParams } from 'react-router-dom';
import { FileDown, Pencil, Printer } from 'lucide-react';
import { Breadcrumbs } from '../components/Breadcrumbs/Breadcrumbs';
import { useChecklist, useAircraftModelStats } from '../hooks/useFleetStats';
import { useCurrentUser } from '../hooks/useCurrentUser';
import { userHasAnyRole } from '../utils/auth';
import { exportChecklistPdf } from '../utils/checklistPdf';
import type { ChecklistType } from '../api/checklists';
import styles from './ChecklistViewerPage.module.css';

const TYPE_LABEL: Record<ChecklistType, string> = {
  normal_ops: 'Normal Operations',
  qrh: 'Quick Reference Handbook',
};

export function ChecklistViewerPage() {
  const { id } = useParams<{ id: string }>();
  const checklistId = id ? Number(id) : null;
  const navigate = useNavigate();
  const { data: currentUser } = useCurrentUser();
  const isAdmin = userHasAnyRole(currentUser, 'admin');

  const { data: checklist, isLoading, error } = useChecklist(checklistId);
  const { data: model } = useAircraftModelStats(checklist?.model_id ?? null);

  if (isLoading) {
    return <p style={{ padding: 'var(--space-5)' }}>Loading checklist…</p>;
  }
  if (error) {
    return (
      <p style={{ padding: 'var(--space-5)', color: 'var(--color-critical)' }}>
        Failed to load checklist: {(error as Error).message}
      </p>
    );
  }
  if (!checklist) return null;

  const modelName = model?.name ?? `Model #${checklist.model_id}`;

  const handlePdf = () => {
    exportChecklistPdf(checklist, modelName);
  };
  const handlePrint = () => {
    window.print();
  };
  const handleEdit = () => {
    navigate(
      `/aircraft-models/${checklist.model_id}/checklists/${checklist.id}/edit`,
    );
  };

  return (
    <div className={styles.page}>
      <div className={styles.breadcrumbs}>
        <Breadcrumbs
          items={[
            { label: 'Aircraft Models', to: '/aircraft-models' },
            { label: modelName, to: `/aircraft-models/${checklist.model_id}` },
            { label: checklist.title },
          ]}
        />
      </div>

      <div className={styles.actionBar}>
        <h1>
          {modelName} · {TYPE_LABEL[checklist.type]}
        </h1>
        <button type="button" className={styles.btnSecondary} onClick={handlePdf}>
          <FileDown size={14} /> Save as PDF
        </button>
        <button type="button" className={styles.btnSecondary} onClick={handlePrint}>
          <Printer size={14} /> Print
        </button>
        {isAdmin && (
          <button type="button" className={styles.btnPrimary} onClick={handleEdit}>
            <Pencil size={14} /> Edit
          </button>
        )}
      </div>

      <article className={styles.docPage}>
        <h2 className={styles.title}>{modelName}</h2>
        <p className={styles.subtitle}>{TYPE_LABEL[checklist.type]}</p>
        <div className={styles.revisionStrip}>
          Rev {checklist.revision}
          {checklist.revision_date ? ` · ${checklist.revision_date}` : ''}
        </div>

        {checklist.sections.map((section, idx) => (
          <section
            key={section.id}
            className={`${styles.section} ${
              section.start_on_new_page && idx > 0 ? styles.sectionStartNewPage : ''
            }`}
          >
            <div className={styles.sectionHeader}>{section.title}</div>
            {section.items.length === 0 ? (
              <div className={styles.emptySection}>(no items)</div>
            ) : (
              <table className={styles.itemTable}>
                <tbody>
                  {section.items.map((item) => (
                    <tr key={item.id}>
                      <td className={styles.colLabel}>{item.label}</td>
                      <td className={styles.colValue}>{item.value}</td>
                      <td className={styles.colResp}>- {item.responsibility}</td>
                    </tr>
                  ))}
                </tbody>
              </table>
            )}
          </section>
        ))}

        <div className={styles.footer}>
          Generated {new Date().toISOString().slice(0, 10)}
        </div>
      </article>
    </div>
  );
}
