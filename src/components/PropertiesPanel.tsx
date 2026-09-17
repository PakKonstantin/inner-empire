/**
 * Editing frontmatter.
 *
 * Each property gets an editor matching its type, so a date is a date field
 * and a checkbox is a checkbox. Writing back goes through the backend's
 * surgical frontmatter replacement, which leaves every byte of the note's body
 * untouched — the user's spacing and line endings survive an edit here.
 */

import { useCallback, useEffect, useState } from 'react';

import { api } from '@/services/api';
import type { Property, PropertyKind, PropertyValue, VaultPath } from '@/types/domain';

export interface PropertiesPanelProps {
  path: VaultPath | null;
  /** The note's parsed properties, refreshed by the caller when it reloads. */
  properties: Property[];
  onChanged: () => void;
}

export function PropertiesPanel({ path, properties, onChanged }: PropertiesPanelProps) {
  const [draft, setDraft] = useState<Property[]>(properties);
  const [adding, setAdding] = useState(false);
  const [newKey, setNewKey] = useState('');
  const [knownKeys, setKnownKeys] = useState<string[]>([]);

  useEffect(() => {
    setDraft(properties);
  }, [properties]);

  useEffect(() => {
    // Existing keys across the vault, so a new property can reuse a name the
    // user already established rather than inventing a near-duplicate.
    api
      .propertyKeys()
      .then((keys) => setKnownKeys(keys.map(([key]) => key)))
      .catch(() => setKnownKeys([]));
  }, []);

  const commit = useCallback(
    async (next: Property[]) => {
      if (!path) return;
      setDraft(next);
      try {
        await api.setProperties(path, next);
        onChanged();
      } catch {
        // Put the panel back to what is on disk rather than showing a value
        // that was not written.
        setDraft(properties);
      }
    },
    [path, properties, onChanged],
  );

  const updateValue = useCallback(
    (key: string, value: PropertyValue) => {
      void commit(draft.map((property) => (property.key === key ? { ...property, value } : property)));
    },
    [draft, commit],
  );

  const removeProperty = useCallback(
    (key: string) => {
      void commit(draft.filter((property) => property.key !== key));
    },
    [draft, commit],
  );

  const addProperty = useCallback(() => {
    const key = newKey.trim();
    if (!key || draft.some((property) => property.key.toLowerCase() === key.toLowerCase())) {
      setAdding(false);
      setNewKey('');
      return;
    }
    void commit([...draft, { key, value: { kind: 'text', value: '' } }]);
    setAdding(false);
    setNewKey('');
  }, [newKey, draft, commit]);

  if (!path) {
    return (
      <div className="ie-panel ie-properties">
        <div className="ie-panel-header">
          <span>Properties</span>
        </div>
        <div className="ie-empty">Open a note to edit its properties.</div>
      </div>
    );
  }

  return (
    <div className="ie-panel ie-properties">
      <div className="ie-panel-header">
        <span>Properties</span>
        <button
          type="button"
          className="ie-icon-button"
          title="Add a property"
          aria-label="Add a property"
          onClick={() => setAdding(true)}
        >
          ＋
        </button>
      </div>

      <div className="ie-panel__body">
        {draft.length === 0 && !adding ? (
          <div className="ie-empty">This note has no properties.</div>
        ) : null}

        {draft.map((property) => (
          <div key={property.key} className="ie-property">
            <div className="ie-property__head">
              <label className="ie-property__key" htmlFor={`property-${property.key}`}>
                {property.key}
              </label>
              <select
                className="ie-property__type"
                aria-label={`Type of ${property.key}`}
                value={property.value.kind}
                onChange={(event) =>
                  updateValue(property.key, convert(property.value, event.target.value as PropertyKind))
                }
              >
                <option value="text">Text</option>
                <option value="number">Number</option>
                <option value="checkbox">Checkbox</option>
                <option value="date">Date</option>
                <option value="datetime">Date and time</option>
                <option value="list">List</option>
              </select>
              <button
                type="button"
                className="ie-icon-button"
                aria-label={`Remove ${property.key}`}
                onClick={() => removeProperty(property.key)}
              >
                ✕
              </button>
            </div>
            <PropertyEditor
              id={`property-${property.key}`}
              value={property.value}
              onChange={(value) => updateValue(property.key, value)}
            />
          </div>
        ))}

        {adding ? (
          <div className="ie-property ie-property--new">
            <input
              className="ie-input"
              autoFocus
              list="ie-known-property-keys"
              placeholder="Property name"
              value={newKey}
              onChange={(event) => setNewKey(event.target.value)}
              onBlur={addProperty}
              onKeyDown={(event) => {
                if (event.key === 'Enter') addProperty();
                if (event.key === 'Escape') {
                  setAdding(false);
                  setNewKey('');
                }
              }}
            />
            <datalist id="ie-known-property-keys">
              {knownKeys.map((key) => (
                <option key={key} value={key} />
              ))}
            </datalist>
          </div>
        ) : null}
      </div>
    </div>
  );
}

interface PropertyEditorProps {
  id: string;
  value: PropertyValue;
  onChange: (value: PropertyValue) => void;
}

function PropertyEditor({ id, value, onChange }: PropertyEditorProps) {
  switch (value.kind) {
    case 'checkbox':
      return (
        <input
          id={id}
          type="checkbox"
          checked={value.value}
          onChange={(event) => onChange({ kind: 'checkbox', value: event.target.checked })}
        />
      );

    case 'number':
      return (
        <input
          id={id}
          className="ie-input"
          type="number"
          value={value.value}
          onChange={(event) => onChange({ kind: 'number', value: Number(event.target.value) })}
        />
      );

    case 'date':
      return (
        <input
          id={id}
          className="ie-input"
          type="date"
          value={value.value}
          onChange={(event) => onChange({ kind: 'date', value: event.target.value })}
        />
      );

    case 'datetime':
      return (
        <input
          id={id}
          className="ie-input"
          type="datetime-local"
          // The stored value is RFC 3339; the input wants it without the zone.
          value={value.value.slice(0, 16)}
          onChange={(event) => onChange({ kind: 'datetime', value: `${event.target.value}:00Z` })}
        />
      );

    case 'list':
      return (
        <div className="ie-property__list">
          {value.value.map((item, index) => (
            <div key={index} className="ie-property__list-item">
              <input
                className="ie-input"
                value={item.kind === 'text' ? item.value : textOf(item)}
                onChange={(event) => {
                  const next = [...value.value];
                  next[index] = { kind: 'text', value: event.target.value };
                  onChange({ kind: 'list', value: next });
                }}
              />
              <button
                type="button"
                className="ie-icon-button"
                aria-label="Remove item"
                onClick={() =>
                  onChange({ kind: 'list', value: value.value.filter((_, i) => i !== index) })
                }
              >
                ✕
              </button>
            </div>
          ))}
          <button
            type="button"
            className="ie-button ie-button--quiet"
            onClick={() => onChange({ kind: 'list', value: [...value.value, { kind: 'text', value: '' }] })}
          >
            Add item
          </button>
        </div>
      );

    case 'object':
      return <div className="ie-property__readonly">Nested values are edited in the note itself.</div>;

    case 'null':
      return (
        <input
          id={id}
          className="ie-input"
          value=""
          placeholder="Empty"
          onChange={(event) => onChange({ kind: 'text', value: event.target.value })}
        />
      );

    default:
      return (
        <input
          id={id}
          className="ie-input"
          value={value.value}
          onChange={(event) => onChange({ kind: 'text', value: event.target.value })}
        />
      );
  }
}

function textOf(value: PropertyValue): string {
  switch (value.kind) {
    case 'text':
    case 'date':
    case 'datetime':
      return value.value;
    case 'number':
      return String(value.value);
    case 'checkbox':
      return String(value.value);
    case 'list':
      return value.value.map(textOf).join(', ');
    default:
      return '';
  }
}

/**
 * Change a property's type, keeping whatever the old value can contribute.
 *
 * Switching to a type the value cannot express gives an empty value of the new
 * type rather than a broken one.
 */
function convert(value: PropertyValue, kind: PropertyKind): PropertyValue {
  const text = textOf(value);
  switch (kind) {
    case 'text':
      return { kind: 'text', value: text };
    case 'number': {
      const number = Number(text);
      return { kind: 'number', value: Number.isFinite(number) ? number : 0 };
    }
    case 'checkbox':
      return { kind: 'checkbox', value: text === 'true' || text === '1' || text === 'yes' };
    case 'date':
      return { kind: 'date', value: /^\d{4}-\d{2}-\d{2}/.test(text) ? text.slice(0, 10) : '' };
    case 'datetime':
      return { kind: 'datetime', value: /^\d{4}-\d{2}-\d{2}/.test(text) ? text : '' };
    case 'list':
      return {
        kind: 'list',
        value: value.kind === 'list' ? value.value : text ? [{ kind: 'text', value: text }] : [],
      };
    default:
      return { kind: 'null' };
  }
}
