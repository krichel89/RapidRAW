import { invoke } from '@tauri-apps/api/core';
import { MediaWikiFields } from '../mediawiki/mediawikiApi';

export const CommonsInvokes = {
  GetSettings: 'commons_get_settings',
  SetSettings: 'commons_set_settings',
  Status: 'commons_status',
  BeginLogin: 'commons_begin_login',
  SubmitCode: 'commons_submit_code',
  CancelLogin: 'commons_cancel_login',
  CompleteLogin: 'commons_complete_login',
  Logout: 'commons_logout',
  TestConnection: 'commons_test_connection',
  PrepareExport: 'commons_prepare_export',
} as const;

/** Mirrors `CommonsSettings` in src-tauri/src/commons/settings.rs. */
export interface CommonsSettings {
  clientId: string;
  editSummary: string;
  trackingCategory: string;
  ignoreWarnings: boolean;
  structuredCapture: boolean;
  defaults: MediaWikiFields;
}

export interface CommonsStatus {
  configured: boolean;
  loggedIn: boolean;
  username: string;
  inCredentialStore: boolean;
  /** The callback the OAuth client has to be registered with. */
  redirectUri: string;
}

export interface CommonsLoginStart {
  url: string;
  /** False when the answer cannot arrive by itself and has to be pasted. */
  loopback: boolean;
}

export const getCommonsSettings = () => invoke<CommonsSettings>(CommonsInvokes.GetSettings);

export const setCommonsSettings = (newSettings: CommonsSettings) =>
  invoke<CommonsSettings>(CommonsInvokes.SetSettings, { newSettings });

export const getCommonsStatus = () => invoke<CommonsStatus>(CommonsInvokes.Status);

export const beginCommonsLogin = () => invoke<CommonsLoginStart>(CommonsInvokes.BeginLogin);

export const submitCommonsCode = (text: string) => invoke<void>(CommonsInvokes.SubmitCode, { text });

export const cancelCommonsLogin = () => invoke<void>(CommonsInvokes.CancelLogin);

export const completeCommonsLogin = () => invoke<CommonsStatus>(CommonsInvokes.CompleteLogin);

export const logoutFromCommons = () => invoke<CommonsStatus>(CommonsInvokes.Logout);

export const testCommonsConnection = () => invoke<string>(CommonsInvokes.TestConnection);

export const prepareCommonsExport = () => invoke<string>(CommonsInvokes.PrepareExport);
