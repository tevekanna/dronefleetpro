// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (C) 2026 Lars Wallström

import { forwardRef, useRef } from 'react';
import type { ChangeEvent, CSSProperties, FocusEventHandler } from 'react';
import { Calendar } from 'lucide-react';

/**
 * IsoDateTimeInput — a datetime input that ALWAYS renders in ISO 8601
 * (YYYY-MM-DDTHH:mm) format, regardless of the browser locale.
 *
 * Why: native `<input type="datetime-local">` displays a browser-locale-
 * dependent format (e.g. dd/mm/yyyy, hh:mm in Chromium browsers on en-*
 * locales). That format is hardcoded Chromium behavior and cannot be
 * overridden via `lang` or CSS. It violates the project-wide ISO 8601
 * display convention.
 *
 * How: the visible field is a plain `<input type="text">` constrained to
 * the ISO datetime pattern, so the user always sees and types
 * YYYY-MM-DDTHH:mm. A hidden `<input type="datetime-local">` provides the
 * native calendar/time picker, triggered by the calendar icon button; its
 * selection is mirrored back onto the visible text input.
 *
 * Value format is "YYYY-MM-DDTHH:mm", matching the HTML5 datetime-local
 * spec; consumers store and read this exact format.
 *
 * Strictly controlled: pass `value` + `onChange`. For react-hook-form,
 * wrap in a `<Controller>` (this component does not register itself).
 */
export type IsoDateTimeInputProps = {
  value: string;
  onChange: (e: ChangeEvent<HTMLInputElement>) => void;
  name?: string;
  id?: string;
  required?: boolean;
  disabled?: boolean;
  min?: string;
  max?: string;
  className?: string;
  style?: CSSProperties;
  placeholder?: string;
  onBlur?: FocusEventHandler<HTMLInputElement>;
  'aria-label'?: string;
  'aria-describedby'?: string;
};

export const IsoDateTimeInput = forwardRef<
  HTMLInputElement,
  IsoDateTimeInputProps
>(function IsoDateTimeInput(props, ref) {
  // HOOKS — keep before any early returns
  const textInputRef = useRef<HTMLInputElement | null>(null);
  const hiddenDateRef = useRef<HTMLInputElement | null>(null);

  const {
    value,
    onChange,
    name,
    id,
    required,
    disabled,
    min,
    max,
    className,
    style,
    placeholder,
    onBlur,
    'aria-label': ariaLabel,
    'aria-describedby': ariaDescribedby,
  } = props;

  const setRefs = (el: HTMLInputElement | null) => {
    textInputRef.current = el;
    if (typeof ref === 'function') ref(el);
    else if (ref) ref.current = el;
  };

  const openPicker = () => {
    const el = hiddenDateRef.current;
    if (!el) return;
    if (typeof el.showPicker === 'function') {
      try {
        el.showPicker();
        return;
      } catch {
        /* fall through */
      }
    }
    el.click();
  };

  // Standard React pattern for programmatic input changes: drive the
  // native value setter then dispatch a bubbling 'input' event so React's
  // synthetic onChange fires with a real event (works for both plain
  // controlled usage and react-hook-form Controller).
  const handleHiddenChange = (e: ChangeEvent<HTMLInputElement>) => {
    const newValue = e.target.value; // HTML5 native ISO "YYYY-MM-DDTHH:mm"
    const textInput = textInputRef.current;
    if (!textInput) return;
    const setter = Object.getOwnPropertyDescriptor(
      window.HTMLInputElement.prototype,
      'value',
    )?.set;
    setter?.call(textInput, newValue);
    textInput.dispatchEvent(new Event('input', { bubbles: true }));
  };

  return (
    <div
      style={{
        position: 'relative',
        display: 'inline-flex',
        alignItems: 'stretch',
        width: '100%',
      }}
    >
      <input
        ref={setRefs}
        type="text"
        pattern="\d{4}-\d{2}-\d{2}T\d{2}:\d{2}"
        inputMode="numeric"
        placeholder={placeholder ?? 'YYYY-MM-DDTHH:mm'}
        className={className}
        style={style}
        value={value}
        onChange={onChange}
        onBlur={onBlur}
        name={name}
        id={id}
        required={required}
        disabled={disabled}
        min={min}
        max={max}
        aria-label={ariaLabel}
        aria-describedby={ariaDescribedby}
      />
      <button
        type="button"
        onClick={openPicker}
        aria-label="Open calendar picker"
        tabIndex={-1}
        disabled={disabled}
        style={{
          position: 'absolute',
          right: '4px',
          top: '50%',
          transform: 'translateY(-50%)',
          background: 'transparent',
          border: 'none',
          padding: '4px',
          cursor: 'pointer',
          color: 'var(--color-text-muted)',
          display: 'flex',
          alignItems: 'center',
        }}
      >
        <Calendar size={16} />
      </button>
      <input
        type="datetime-local"
        ref={hiddenDateRef}
        value={value || ''}
        onChange={handleHiddenChange}
        tabIndex={-1}
        aria-hidden="true"
        min={min}
        max={max}
        style={{
          position: 'absolute',
          opacity: 0,
          pointerEvents: 'none',
          width: 0,
          height: 0,
        }}
      />
    </div>
  );
});
