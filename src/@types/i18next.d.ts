import 'i18next';
import en from '../i18n/locales/en.json';
import extraEn from '../i18n/extra/en.json';

declare module 'i18next' {
  interface CustomTypeOptions {
    defaultNS: 'translation';
    resources: {
      translation: typeof en & typeof extraEn;
    };
  }
}
