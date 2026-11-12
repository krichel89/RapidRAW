import { useCallback, useEffect, useState } from 'react';
import { listen } from '@tauri-apps/api/event';
import { open } from '@tauri-apps/plugin-shell';
import { useTranslation } from 'react-i18next';
import { toast } from 'react-toastify';
import { ChevronDown, ChevronRight } from 'lucide-react';
import Button from '../ui/Button';
import Input from '../ui/Input';
import Switch from '../ui/Switch';
import Text from '../ui/Text';
import { TextColors, TextVariants, TextWeights } from '../../types/typography';
import MediaWikiFieldsEditor from '../mediawiki/MediaWikiFieldsEditor';
import { applyPatchLocally, countFilled, MediaWikiPatch } from '../mediawiki/mediawikiApi';
import {
  beginCommonsLogin,
  cancelCommonsLogin,
  CommonsLoginStart,
  CommonsSettings,
  CommonsStatus,
  completeCommonsLogin,
  getCommonsSettings,
  getCommonsStatus,
  logoutFromCommons,
  setCommonsSettings,
  submitCommonsCode,
  testCommonsConnection,
} from './commonsApi';

const REGISTRATION_URL = 'https://meta.wikimedia.org/wiki/Special:OAuthConsumerRegistration/propose/oauth2';

function Collapsible({ title, children }: { title: string; children: React.ReactNode }) {
  const [isOpen, setIsOpen] = useState(false);
  return (
    <div className="rounded-lg border border-surface/70">
      <button
        onClick={() => setIsOpen(!isOpen)}
        className="w-full flex items-center justify-between px-3 py-2 hover:bg-card-active transition-colors rounded-lg"
      >
        <Text as="span" variant={TextVariants.small} color={TextColors.primary} weight={TextWeights.semibold}>
          {title}
        </Text>
        <Text color={TextColors.secondary}>{isOpen ? <ChevronDown size={14} /> : <ChevronRight size={14} />}</Text>
      </button>
      {isOpen && <div className="flex flex-col gap-3 px-3 pb-3 pt-1">{children}</div>}
    </div>
  );
}

/** The export area for Wikimedia Commons: sign-in, options, defaults for the upload. */
export default function CommonsExportOptions({ disabled }: { disabled?: boolean }) {
  const { t } = useTranslation();
  const [settings, setSettings] = useState<CommonsSettings | null>(null);
  const [status, setStatus] = useState<CommonsStatus | null>(null);
  const [login, setLogin] = useState<CommonsLoginStart | null>(null);
  const [pasted, setPasted] = useState('');
  const [busy, setBusy] = useState(false);

  const reload = useCallback(async () => {
    try {
      const [loadedSettings, loadedStatus] = await Promise.all([getCommonsSettings(), getCommonsStatus()]);
      setSettings(loadedSettings);
      setStatus(loadedStatus);
    } catch (error) {
      console.error(`Failed to load the Commons settings: ${error}`);
    }
  }, []);

  useEffect(() => {
    reload();
  }, [reload]);

  useEffect(() => {
    const unlisten = listen<{ fileName: string; commonsName: string }>('commons-upload', (event) => {
      toast.success(t('commons.export.uploaded', { name: event.payload.commonsName }));
    });
    return () => {
      unlisten.then((stop) => stop());
    };
  }, [t]);

  const save = async (next: CommonsSettings) => {
    setSettings(next);
    try {
      setSettings(await setCommonsSettings(next));
      setStatus(await getCommonsStatus());
    } catch (error) {
      toast.error(t('commons.settings.saveFailed', { error: String(error) }));
      await reload();
    }
  };

  const startLogin = async () => {
    setBusy(true);
    setPasted('');
    try {
      const started = await beginCommonsLogin();
      setLogin(started);
      await open(started.url);
      setStatus(await completeCommonsLogin());
    } catch (error) {
      toast.error(String(error));
    } finally {
      setLogin(null);
      setBusy(false);
    }
  };

  const cancelLogin = async () => {
    await cancelCommonsLogin();
  };

  const submitPasted = async () => {
    try {
      await submitCommonsCode(pasted);
      setPasted('');
    } catch (error) {
      toast.error(String(error));
    }
  };

  const logout = async () => {
    try {
      setStatus(await logoutFromCommons());
    } catch (error) {
      toast.error(String(error));
    }
  };

  const testConnection = async () => {
    try {
      toast.success(t('commons.login.testOk', { name: await testCommonsConnection() }));
    } catch (error) {
      toast.error(t('commons.login.testFailed', { error: String(error) }));
    }
  };

  if (!settings || !status) return null;

  const defaultsCount = countFilled(settings.defaults);

  return (
    <div className="space-y-3 pl-2 border-l-2 border-surface">
      {status.loggedIn ? (
        <div className="flex items-center justify-between gap-2">
          <Text variant={TextVariants.small} color={TextColors.primary}>
            {t('commons.login.signedInAs', { name: status.username })}
          </Text>
          <div className="flex gap-2">
            <button
              className="text-xs text-text-secondary hover:text-accent disabled:opacity-30"
              disabled={disabled}
              onClick={testConnection}
            >
              {t('commons.login.test')}
            </button>
            <button
              className="text-xs text-text-secondary hover:text-accent disabled:opacity-30"
              disabled={disabled}
              onClick={logout}
            >
              {t('commons.login.signOut')}
            </button>
          </div>
        </div>
      ) : (
        <div className="flex flex-col gap-2">
          <Text variant={TextVariants.small} color={TextColors.secondary}>
            {status.configured ? t('commons.login.notSignedIn') : t('commons.login.notSetUp')}
          </Text>
          {status.configured && !login && (
            <Button onClick={startLogin} disabled={disabled || busy}>
              {t('commons.login.signIn')}
            </Button>
          )}
          {login && (
            <div className="flex flex-col gap-2">
              <Text variant={TextVariants.small} color={TextColors.secondary}>
                {login.loopback ? t('commons.login.waiting') : t('commons.login.portBusy')}
              </Text>
              <div className="flex items-center gap-2">
                <Input
                  bgClassName="bg-bg-primary"
                  value={pasted}
                  placeholder={t('commons.login.pastePlaceholder')}
                  onChange={(e) => setPasted(e.target.value)}
                  onKeyDown={(e) => e.key === 'Enter' && submitPasted()}
                />
                <Button onClick={submitPasted} disabled={!pasted.trim()}>
                  {t('commons.login.submitCode')}
                </Button>
              </div>
              <div className="flex gap-3">
                <button className="text-xs text-text-secondary hover:text-accent" onClick={() => open(login.url)}>
                  {t('commons.login.openAgain')}
                </button>
                <button className="text-xs text-text-secondary hover:text-accent" onClick={cancelLogin}>
                  {t('commons.login.cancel')}
                </button>
              </div>
            </div>
          )}
        </div>
      )}

      <Collapsible title={t('commons.settings.title')}>
        <div className="flex flex-col gap-1">
          <Text variant={TextVariants.small} color={TextColors.secondary} weight={TextWeights.medium}>
            {t('commons.settings.clientId')}
          </Text>
          <Input
            bgClassName="bg-bg-primary"
            value={settings.clientId}
            disabled={disabled}
            onChange={(e) => setSettings({ ...settings, clientId: e.target.value })}
            onBlur={() => save(settings)}
          />
          <Text variant={TextVariants.small} color={TextColors.secondary}>
            {t('commons.settings.clientIdHelp')}
          </Text>
          <Text variant={TextVariants.small} color={TextColors.primary} className="select-all break-all">
            {status.redirectUri}
          </Text>
          <button
            className="self-start text-xs text-text-secondary hover:text-accent"
            onClick={() => open(REGISTRATION_URL)}
          >
            {t('commons.settings.register')}
          </button>
        </div>
        <div className="flex flex-col gap-1">
          <Text variant={TextVariants.small} color={TextColors.secondary} weight={TextWeights.medium}>
            {t('commons.settings.editSummary')}
          </Text>
          <Input
            bgClassName="bg-bg-primary"
            value={settings.editSummary}
            disabled={disabled}
            onChange={(e) => setSettings({ ...settings, editSummary: e.target.value })}
            onBlur={() => save(settings)}
          />
        </div>
        <div className="flex flex-col gap-1">
          <Text variant={TextVariants.small} color={TextColors.secondary} weight={TextWeights.medium}>
            {t('commons.settings.trackingCategory')}
          </Text>
          <Input
            bgClassName="bg-bg-primary"
            value={settings.trackingCategory}
            placeholder="Uploaded with RapidRAW"
            disabled={disabled}
            onChange={(e) => setSettings({ ...settings, trackingCategory: e.target.value })}
            onBlur={() => save(settings)}
          />
        </div>
        <Switch
          label={t('commons.settings.structuredCapture')}
          tooltip={t('commons.settings.structuredCaptureDesc')}
          checked={settings.structuredCapture}
          onChange={(structuredCapture) => save({ ...settings, structuredCapture })}
          disabled={disabled}
        />
        <Switch
          label={t('commons.settings.ignoreWarnings')}
          tooltip={t('commons.settings.ignoreWarningsDesc')}
          checked={settings.ignoreWarnings}
          onChange={(ignoreWarnings) => save({ ...settings, ignoreWarnings })}
          disabled={disabled}
        />
      </Collapsible>

      <Collapsible title={t('commons.defaults.title', { count: defaultsCount })}>
        <Text variant={TextVariants.small} color={TextColors.secondary}>
          {t('commons.defaults.help')}
        </Text>
        <MediaWikiFieldsEditor
          records={[settings.defaults]}
          level="defaults"
          disabled={disabled}
          onPatch={(patch: MediaWikiPatch) =>
            save({ ...settings, defaults: applyPatchLocally(settings.defaults, patch) })
          }
        />
      </Collapsible>
    </div>
  );
}
