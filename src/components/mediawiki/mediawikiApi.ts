import { invoke } from '@tauri-apps/api/core';

export const MediaWikiInvokes = {
  GetMetadata: 'mediawiki_get_metadata',
  SetMetadata: 'mediawiki_set_metadata',
} as const;

/**
 * The MediaWiki metadata of one image (or the defaults for a whole upload).
 * Mirrors `MediaWikiFields` in src-tauri/src/mediawiki/model.rs; empty fields
 * are simply absent.
 */
export interface MediaWikiFields {
  targetFilename?: string;
  date?: string;
  description?: string;
  captions?: Record<string, string>;
  templates?: string;
  baseDescription?: string;
  author?: string;
  photographerCategory?: string;
  creator?: string;
  source?: string;
  permission?: string;
  license?: string;
  copyright?: string;
  depicts?: string[];
  depictsOverride?: string;
  ifNoDepicts?: string;
  categories?: string[];
  createdDuring?: string;
  cameraPosition?: string;
  objectPosition?: string;
  galleryPage?: string;
  gallerySuffix?: string;
  extraWikitext?: string;
  statements?: Record<string, string>;
  composer?: string;
  composerDied?: string;
  compositionYear?: string;
  compositionLicense?: string;
  work?: string;
  recordedBy?: string;
  recordingTechnique?: string;
  sourceTemplate?: string;
  recordingLicense?: string;
  otherVersions?: string;
  instrument?: string;
  period?: string;
  country?: string;
}

export type MediaWikiKey = keyof MediaWikiFields;

/** Only the named fields change; `null`, `''`, `[]` and `{}` clear one. */
export type MediaWikiPatch = { [K in MediaWikiKey]?: MediaWikiFields[K] | null };

export const getMediaWikiMetadata = (paths: string[]) =>
  invoke<MediaWikiFields[]>(MediaWikiInvokes.GetMetadata, { paths });

export const setMediaWikiMetadata = (paths: string[], patch: MediaWikiPatch) =>
  invoke<void>(MediaWikiInvokes.SetMetadata, { paths, patch });

/** How a field is edited. */
export type FieldKind = 'text' | 'area' | 'list' | 'map';

/** Where a field belongs: on an image, in the defaults for the whole upload, or both. */
export type FieldLevel = 'image' | 'defaults' | 'both';

export type FieldGroup = 'file' | 'description' | 'rights' | 'structured' | 'wikitext' | 'music';

export interface FieldSpec {
  key: MediaWikiKey;
  kind: FieldKind;
  level: FieldLevel;
  group: FieldGroup;
  /** Shown as placeholder / example. */
  example?: string;
}

/**
 * Every field once, in the order of Cammello's MediaWiki tab. Labels are
 * `mediawiki.fields.<key>` in the translations.
 */
export const FIELD_SPECS: FieldSpec[] = [
  {
    key: 'targetFilename',
    kind: 'text',
    level: 'image',
    group: 'file',
    example: 'Anna Meier at the Berlinale 2026.jpg',
  },
  { key: 'date', kind: 'text', level: 'both', group: 'file', example: '2026-02-20' },
  { key: 'baseDescription', kind: 'area', level: 'defaults', group: 'description' },
  { key: 'description', kind: 'area', level: 'image', group: 'description', example: '{{en|1=Anna Meier}}' },
  { key: 'captions', kind: 'map', level: 'both', group: 'description' },
  { key: 'author', kind: 'text', level: 'both', group: 'rights' },
  { key: 'photographerCategory', kind: 'text', level: 'both', group: 'rights' },
  { key: 'creator', kind: 'text', level: 'both', group: 'rights', example: 'Q…' },
  { key: 'source', kind: 'text', level: 'both', group: 'rights', example: '{{own}}' },
  { key: 'permission', kind: 'text', level: 'both', group: 'rights' },
  { key: 'license', kind: 'text', level: 'both', group: 'rights', example: '{{Cc-by-sa-4.0}}' },
  { key: 'copyright', kind: 'text', level: 'both', group: 'rights', example: 'Q73566113' },
  { key: 'depicts', kind: 'list', level: 'both', group: 'structured', example: 'Q42' },
  { key: 'ifNoDepicts', kind: 'text', level: 'both', group: 'structured' },
  { key: 'depictsOverride', kind: 'text', level: 'both', group: 'structured' },
  { key: 'categories', kind: 'list', level: 'both', group: 'structured', example: 'Berlinale 2026' },
  { key: 'createdDuring', kind: 'text', level: 'both', group: 'structured', example: 'Q…' },
  { key: 'cameraPosition', kind: 'text', level: 'both', group: 'structured', example: '52.5200, 13.4050' },
  { key: 'objectPosition', kind: 'text', level: 'both', group: 'structured', example: '52.5200, 13.4050' },
  { key: 'galleryPage', kind: 'text', level: 'defaults', group: 'structured' },
  { key: 'gallerySuffix', kind: 'text', level: 'defaults', group: 'structured' },
  { key: 'statements', kind: 'map', level: 'both', group: 'structured' },
  { key: 'templates', kind: 'area', level: 'both', group: 'wikitext', example: '{{Do not crop}}' },
  { key: 'otherVersions', kind: 'text', level: 'both', group: 'wikitext' },
  { key: 'extraWikitext', kind: 'area', level: 'both', group: 'wikitext' },
  { key: 'compositionYear', kind: 'text', level: 'both', group: 'music' },
  { key: 'sourceTemplate', kind: 'text', level: 'both', group: 'music' },
  { key: 'composer', kind: 'text', level: 'both', group: 'music' },
  { key: 'recordedBy', kind: 'text', level: 'both', group: 'music' },
  { key: 'recordingTechnique', kind: 'text', level: 'both', group: 'music' },
  { key: 'composerDied', kind: 'text', level: 'both', group: 'music' },
  { key: 'compositionLicense', kind: 'text', level: 'both', group: 'music' },
  { key: 'recordingLicense', kind: 'text', level: 'both', group: 'music' },
  { key: 'instrument', kind: 'text', level: 'both', group: 'music' },
  { key: 'period', kind: 'text', level: 'both', group: 'music' },
  { key: 'work', kind: 'text', level: 'both', group: 'music' },
  { key: 'country', kind: 'text', level: 'both', group: 'music' },
];

export const FIELD_GROUPS: FieldGroup[] = ['file', 'description', 'rights', 'structured', 'wikitext', 'music'];

/** The value of a field as the editor shows it. */
export const emptyValue = (kind: FieldKind): string | string[] | Record<string, string> =>
  kind === 'list' ? [] : kind === 'map' ? {} : '';

/** Whether a record carries any value at all. */
export const countFilled = (record: MediaWikiFields | undefined): number =>
  record
    ? FIELD_SPECS.filter((spec) => {
        const value = record[spec.key];
        if (Array.isArray(value)) return value.length > 0;
        if (value && typeof value === 'object') return Object.keys(value).length > 0;
        return !!value;
      }).length
    : 0;

/**
 * The value all records share for a field, or `mixed` when they differ.
 * Records are compared by content, so an absent field equals an empty one.
 */
export function commonValue(
  records: MediaWikiFields[],
  spec: FieldSpec,
): { mixed: boolean; value: string | string[] | Record<string, string> } {
  const empty = emptyValue(spec.kind);
  if (records.length === 0) return { mixed: false, value: empty };
  const first = (records[0][spec.key] ?? empty) as string | string[] | Record<string, string>;
  const firstJson = JSON.stringify(first);
  const mixed = records.some((record) => JSON.stringify(record[spec.key] ?? empty) !== firstJson);
  return { mixed, value: mixed ? empty : first };
}

/**
 * Apply a patch to a record in the browser, with the same meaning as on the
 * backend: named fields change, `null`, `''`, `[]` and `{}` clear one.
 */
export function applyPatchLocally(record: MediaWikiFields, patch: MediaWikiPatch): MediaWikiFields {
  const next: Record<string, unknown> = { ...record };
  for (const [key, value] of Object.entries(patch)) {
    const isEmpty =
      value === null ||
      value === undefined ||
      value === '' ||
      (Array.isArray(value) && value.length === 0) ||
      (typeof value === 'object' && !Array.isArray(value) && Object.keys(value).length === 0);
    if (isEmpty) delete next[key];
    else next[key] = value;
  }
  return next as MediaWikiFields;
}
