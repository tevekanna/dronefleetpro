// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (C) 2026 Lars Wallström

import { useState } from 'react';
import { useMutation, useQuery, useQueryClient } from '@tanstack/react-query';
import { ChevronRight, Pencil, Trash2 } from 'lucide-react';
import {
  fetchInstructionTemplateDetail,
  deactivateInstructionTemplate,
} from '../../api/maintenanceInstructionTemplates';
import { showToast } from '../../store/toastStore';
import type { TemplateListItem } from '../../types/maintenanceInstructionTemplate';
import { EditChecklistModal } from './EditChecklistModal';
import styles from './InstructionTemplateList.module.css';

/**
 * Inline detail panel — only mounted when its parent row is expanded, so the
 * detail query fires lazily (once per expansion). All hooks live at the top of
 * this component, before any conditional return.
 */
function TemplateDetailPanel({ id }: { id: number }) {
  const { data, isLoading, error } = useQuery({
    queryKey: ['instruction-template-detail', id],
    queryFn: () => fetchInstructionTemplateDetail(id),
  });

  if (isLoading) {
    return <div className={styles.loadingDetail}>Loading checklist...</div>;
  }

  if (error) {
    return (
      <div className={styles.loadingDetail}>
        Failed to load checklist: {(error as Error).message}
      </div>
    );
  }

  if (!data || data.sections.length === 0) {
    return <div className={styles.loadingDetail}>This template has no checklist items.</div>;
  }

  return (
    <div className={styles.detail}>
      {data.sections.map((section) => (
        <div key={`${section.section_order}-${section.section_name}`}>
          <div className={styles.sectionTitle}>{section.section_name}</div>
          {section.items.map((item) => (
            <div key={item.id} className={styles.itemRow}>
              <span className={styles.itemNumber}>{item.item_order}.</span>
              <div className={styles.itemBody}>
                <div className={styles.itemText}>{item.item_text}</div>
                {item.reference_manual && (
                  <div className={styles.itemRef}>{item.reference_manual}</div>
                )}
              </div>
              <span
                className={`${styles.itemBadge} ${
                  item.is_required ? styles.badgeRequired : styles.badgeOptional
                }`}
              >
                {item.is_required ? 'Required' : 'Optional'}
              </span>
            </div>
          ))}
        </div>
      ))}
    </div>
  );
}

interface Props {
  templates: TemplateListItem[];
  isAdmin: boolean;
}

/**
 * Renders a flat list of inspection-checklist templates with expandable
 * detail and an admin-only deactivate action. Grouping (by model, generic
 * last) is the caller's concern — pass one group's items at a time on the
 * page, or a single model's items on the workspace tab. Owns the deactivate
 * mutation so both consumers behave identically; invalidates
 * `['instruction-templates']` on success.
 */
export function InstructionTemplateList({ templates, isAdmin }: Props) {
  const [expandedId, setExpandedId] = useState<number | null>(null);
  const [editingId, setEditingId] = useState<number | null>(null);
  const queryClient = useQueryClient();

  const deactivateMutation = useMutation({
    mutationFn: (id: number) => deactivateInstructionTemplate(id),
    onSuccess: () => {
      queryClient.invalidateQueries({ queryKey: ['instruction-templates'] });
      showToast('Template deactivated.', 'info');
    },
    onError: (e: Error) => showToast(`Failed to deactivate: ${e.message}`, 'error'),
  });

  return (
    <>
      {templates.map((template) => {
        const expanded = expandedId === template.id;
        return (
          <div key={template.id}>
            <div
              className={`${styles.templateRow} ${expanded ? styles.expanded : ''}`}
              role="button"
              tabIndex={0}
              aria-expanded={expanded}
              onClick={() => setExpandedId(expanded ? null : template.id)}
              onKeyDown={(e) => {
                if (e.key === 'Enter' || e.key === ' ') {
                  e.preventDefault();
                  setExpandedId(expanded ? null : template.id);
                }
              }}
            >
              <span className={styles.templateName}>{template.name}</span>
              <span
                className={`${styles.badge} ${
                  template.source === 'manufacturer'
                    ? styles.badgeManufacturer
                    : styles.badgeCustom
                }`}
              >
                {template.source === 'manufacturer' ? 'Manufacturer' : 'Custom'}
              </span>
              <span className={styles.meta}>Rev. {template.revision}</span>
              <span className={styles.meta}>{template.item_count} items</span>
              <ChevronRight
                size={18}
                className={`${styles.chevron} ${expanded ? styles.chevronOpen : ''}`}
              />
              {isAdmin && template.source === 'custom' && (
                <button
                  type="button"
                  className={styles.editBtn}
                  onClick={(e) => {
                    e.stopPropagation();
                    setEditingId(template.id);
                  }}
                  title="Edit checklist"
                >
                  <Pencil size={13} />
                  Edit
                </button>
              )}
              {isAdmin && template.source === 'custom' && (
                <button
                  type="button"
                  className={styles.deactivateBtn}
                  onClick={(e) => {
                    e.stopPropagation();
                    if (
                      window.confirm(
                        `Deactivate "${template.name}"? It will no longer appear in checklists.`,
                      )
                    ) {
                      deactivateMutation.mutate(template.id);
                    }
                  }}
                  title="Deactivate template"
                >
                  <Trash2 size={13} />
                  Deactivate
                </button>
              )}
            </div>
            {expanded && <TemplateDetailPanel id={template.id} />}
          </div>
        );
      })}

      {editingId !== null && (
        <EditChecklistModal templateId={editingId} onClose={() => setEditingId(null)} />
      )}
    </>
  );
}
