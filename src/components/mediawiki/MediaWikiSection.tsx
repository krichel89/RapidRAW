import { useCallback, useEffect, useMemo, useState } from 'react';
import { useTranslation } from 'react-i18next';
import { toast } from 'react-toastify';
import { ChevronDown, ChevronRight, Globe } from 'lucide-react';
import Text from '../ui/Text';
import { TextColors, TextVariants } from '../../types/typography';
import MediaWikiFieldsEditor from './MediaWikiFieldsEditor';
import { getMediaWikiMetadata, MediaWikiFields, MediaWikiPatch, setMediaWikiMetadata } from './mediawikiApi';

interface MediaWikiSectionProps {
  /** The images shown (the selection). */
  paths: string[];
  /** The images to write to, if that differs from `paths` (grouped images). */
  resolveWritePaths?: () => string[];
}

/** The MediaWiki metadata of the selected images, in the metadata panel. */
export default function MediaWikiSection({ paths, resolveWritePaths }: MediaWikiSectionProps) {
  const { t } = useTranslation();
  const [isOpen, setIsOpen] = useState(false);
  const [records, setRecords] = useState<MediaWikiFields[]>([]);
  const signature = paths.join('\n');
  const stablePaths = useMemo(() => paths, [signature]);

  const load = useCallback(async () => {
    if (stablePaths.length === 0) return [];
    return getMediaWikiMetadata(stablePaths);
  }, [stablePaths]);

  useEffect(() => {
    if (!isOpen) return;
    let cancelled = false;
    load()
      .then((loaded) => !cancelled && setRecords(loaded))
      .catch((error) => console.error(`Failed to load the MediaWiki metadata: ${error}`));
    return () => {
      cancelled = true;
    };
  }, [isOpen, load]);

  const handlePatch = async (patch: MediaWikiPatch) => {
    try {
      await setMediaWikiMetadata(resolveWritePaths ? resolveWritePaths() : stablePaths, patch);
    } catch (error) {
      toast.error(t('mediawiki.saveFailed', { error: String(error) }));
    }
    // Reload either way: after a refusal the fields return to what is stored.
    try {
      setRecords(await load());
    } catch (error) {
      console.error(`Failed to reload the MediaWiki metadata: ${error}`);
    }
  };

  return (
    <div>
      <Text variant={TextVariants.heading} className="mb-3">
        {t('mediawiki.title')}
      </Text>
      <div className="bg-surface rounded-xl overflow-hidden">
        <button
          onClick={() => setIsOpen(!isOpen)}
          className="w-full flex items-center justify-between p-3 hover:bg-card-active transition-colors"
        >
          <Text as="span" variant={TextVariants.label} color={TextColors.primary} className="flex items-center gap-2">
            <Globe size={16} /> {t('mediawiki.commonsDetails')}
          </Text>
          <Text color={TextColors.secondary}>{isOpen ? <ChevronDown size={16} /> : <ChevronRight size={16} />}</Text>
        </button>
        {isOpen && (
          <div className="px-3 pb-3 pt-2 border-t border-surface/50 flex flex-col gap-3">
            {stablePaths.length > 1 && (
              <Text variant={TextVariants.small} color={TextColors.secondary}>
                {t('mediawiki.multiSelection', { count: stablePaths.length })}
              </Text>
            )}
            <MediaWikiFieldsEditor records={records} level="image" onPatch={handlePatch} />
          </div>
        )}
      </div>
    </div>
  );
}
