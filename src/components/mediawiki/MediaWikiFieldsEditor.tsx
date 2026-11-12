import { useEffect, useState } from 'react';
import { useTranslation } from 'react-i18next';
import { ChevronDown, ChevronRight, Plus, X } from 'lucide-react';
import clsx from 'clsx';
import Input from '../ui/Input';
import Text from '../ui/Text';
import { TextColors, TextVariants, TextWeights } from '../../types/typography';
import {
  commonValue,
  FIELD_GROUPS,
  FIELD_SPECS,
  FieldGroup,
  FieldSpec,
  MediaWikiFields,
  MediaWikiPatch,
} from './mediawikiApi';

type MapRow = { key: string; value: string };
type Draft = string | MapRow[];

const AREA_CLASS = clsx(
  'w-full rounded-md border px-3 py-2 text-sm resize-y min-h-[4.5rem]',
  'border-border-color bg-bg-primary text-text-primary placeholder:text-text-secondary',
  'focus-visible:outline-hidden focus-visible:ring-2 focus-visible:ring-accent',
  'disabled:cursor-not-allowed disabled:opacity-50',
);

const toDraft = (spec: FieldSpec, value: unknown): Draft => {
  if (spec.kind === 'list') return ((value as string[]) ?? []).join('\n');
  if (spec.kind === 'map')
    return Object.entries((value as Record<string, string>) ?? {}).map(([key, v]) => ({ key, value: v }));
  return (value as string) ?? '';
};

const fromDraft = (spec: FieldSpec, draft: Draft): string | string[] | Record<string, string> => {
  if (spec.kind === 'list')
    return (draft as string)
      .split('\n')
      .map((line) => line.trim())
      .filter(Boolean);
  if (spec.kind === 'map')
    return Object.fromEntries(
      (draft as MapRow[]).filter((row) => row.key.trim() && row.value.trim()).map((row) => [row.key.trim(), row.value]),
    );
  return draft as string;
};

interface FieldEditorProps {
  spec: FieldSpec;
  records: MediaWikiFields[];
  onPatch(patch: MediaWikiPatch): void;
  disabled?: boolean;
}

function FieldEditor({ spec, records, onPatch, disabled }: FieldEditorProps) {
  const { t } = useTranslation();
  const { mixed, value } = commonValue(records, spec);
  const signature = JSON.stringify([mixed, value]);
  const [draft, setDraft] = useState<Draft>(() => toDraft(spec, value));
  const [dirty, setDirty] = useState(false);

  // A change from outside (other image selected, saved value, failed save) replaces the draft.
  useEffect(() => {
    setDraft(toDraft(spec, value));
    setDirty(false);
  }, [signature, spec.key]);

  const commit = (next: Draft = draft) => {
    if (!dirty) return;
    setDirty(false);
    onPatch({ [spec.key]: fromDraft(spec, next) } as MediaWikiPatch);
  };

  const edit = (next: Draft) => {
    setDraft(next);
    setDirty(true);
  };

  const label = t(`mediawiki.fields.${spec.key}`);
  const placeholder = mixed ? t('mediawiki.mixed') : (spec.example ?? '');

  return (
    <div className="flex flex-col gap-1">
      <Text variant={TextVariants.small} color={TextColors.secondary} weight={TextWeights.medium}>
        {label}
      </Text>

      {spec.kind === 'text' && (
        <Input
          value={draft as string}
          placeholder={placeholder}
          bgClassName="bg-bg-primary"
          disabled={disabled}
          onChange={(e) => edit(e.target.value)}
          onBlur={() => commit()}
          onKeyDown={(e) => e.key === 'Enter' && e.currentTarget.blur()}
        />
      )}

      {(spec.kind === 'area' || spec.kind === 'list') && (
        <textarea
          className={AREA_CLASS}
          value={draft as string}
          placeholder={spec.kind === 'list' && !mixed ? `${placeholder}\n${t('mediawiki.onePerLine')}` : placeholder}
          disabled={disabled}
          rows={spec.kind === 'list' ? 3 : 4}
          spellCheck={false}
          onChange={(e) => edit(e.target.value)}
          onBlur={() => commit()}
        />
      )}

      {spec.kind === 'map' && (
        <div className="flex flex-col gap-1.5">
          {mixed && !dirty && (draft as MapRow[]).length === 0 && (
            <Text variant={TextVariants.small} color={TextColors.secondary} className="italic">
              {t('mediawiki.mixed')}
            </Text>
          )}
          {(draft as MapRow[]).map((row, index) => (
            <div key={index} className="flex items-center gap-1.5">
              <Input
                className="w-20 shrink-0"
                bgClassName="bg-bg-primary"
                value={row.key}
                placeholder={spec.key === 'captions' ? 'de' : 'P123'}
                disabled={disabled}
                onChange={(e) =>
                  edit((draft as MapRow[]).map((r, i) => (i === index ? { ...r, key: e.target.value } : r)))
                }
                onBlur={() => commit()}
              />
              <Input
                bgClassName="bg-bg-primary"
                value={row.value}
                placeholder={spec.key === 'captions' ? t('mediawiki.captionText') : t('mediawiki.statementValue')}
                disabled={disabled}
                onChange={(e) =>
                  edit((draft as MapRow[]).map((r, i) => (i === index ? { ...r, value: e.target.value } : r)))
                }
                onBlur={() => commit()}
              />
              <button
                className="text-text-secondary hover:text-text-primary disabled:opacity-30"
                disabled={disabled}
                data-tooltip={t('mediawiki.removeRow')}
                onClick={() => {
                  const next = (draft as MapRow[]).filter((_, i) => i !== index);
                  setDraft(next);
                  setDirty(false);
                  onPatch({ [spec.key]: fromDraft(spec, next) } as MediaWikiPatch);
                }}
              >
                <X size={14} />
              </button>
            </div>
          ))}
          <button
            className="self-start flex items-center gap-1 text-xs text-text-secondary hover:text-accent disabled:opacity-30"
            disabled={disabled}
            onClick={() => setDraft([...(draft as MapRow[]), { key: '', value: '' }])}
          >
            <Plus size={12} /> {t('mediawiki.addRow')}
          </button>
        </div>
      )}
    </div>
  );
}

interface MediaWikiFieldsEditorProps {
  records: MediaWikiFields[];
  /** `image`: one image or a selection; `defaults`: the values for a whole upload. */
  level: 'image' | 'defaults';
  onPatch(patch: MediaWikiPatch): void;
  disabled?: boolean;
}

const OPEN_BY_DEFAULT: FieldGroup[] = ['file', 'description'];

export default function MediaWikiFieldsEditor({ records, level, onPatch, disabled }: MediaWikiFieldsEditorProps) {
  const { t } = useTranslation();
  const [open, setOpen] = useState<Set<FieldGroup>>(new Set(OPEN_BY_DEFAULT));

  const toggle = (group: FieldGroup) =>
    setOpen((current) => {
      const next = new Set(current);
      if (!next.delete(group)) next.add(group);
      return next;
    });

  return (
    <div className="flex flex-col gap-2">
      {FIELD_GROUPS.map((group) => {
        const specs = FIELD_SPECS.filter(
          (spec) => spec.group === group && (spec.level === 'both' || spec.level === level),
        );
        if (specs.length === 0) return null;
        const isOpen = open.has(group);
        return (
          <div key={group} className="rounded-lg border border-surface/70">
            <button
              onClick={() => toggle(group)}
              className="w-full flex items-center justify-between px-3 py-2 hover:bg-card-active transition-colors rounded-lg"
            >
              <Text as="span" variant={TextVariants.small} color={TextColors.primary} weight={TextWeights.semibold}>
                {t(`mediawiki.groups.${group}`)}
              </Text>
              <Text color={TextColors.secondary}>
                {isOpen ? <ChevronDown size={14} /> : <ChevronRight size={14} />}
              </Text>
            </button>
            {isOpen && (
              <div className="flex flex-col gap-3 px-3 pb-3 pt-1">
                {specs.map((spec) => (
                  <FieldEditor key={spec.key} spec={spec} records={records} onPatch={onPatch} disabled={disabled} />
                ))}
              </div>
            )}
          </div>
        );
      })}
    </div>
  );
}
