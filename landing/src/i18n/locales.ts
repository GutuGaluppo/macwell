import en from '../content/en.json';
import ptBr from '../content/pt-br.json';
import es from '../content/es.json';
import de from '../content/de.json';

export const LOCALES = ['en', 'pt-br', 'es', 'de'] as const;
export type Locale = (typeof LOCALES)[number];
export type Content = typeof en;

export const content: Record<Locale, Content> = { en, 'pt-br': ptBr, es, de };

export const meta: Record<Locale, { name: string; htmlLang: string; ogLocale: string; bcp47: string }> = {
  en: { name: 'English', htmlLang: 'en', ogLocale: 'en_US', bcp47: 'en-US' },
  'pt-br': { name: 'Português (Brasil)', htmlLang: 'pt-BR', ogLocale: 'pt_BR', bcp47: 'pt-BR' },
  es: { name: 'Español', htmlLang: 'es', ogLocale: 'es_ES', bcp47: 'es-ES' },
  de: { name: 'Deutsch', htmlLang: 'de', ogLocale: 'de_DE', bcp47: 'de-DE' },
};

/** Banner shown on a page when the browser prefers another locale. Text is in the suggested language. */
export const banner: Record<Locale, { text: string; action: string; dismiss: string }> = {
  en: { text: 'This page is also available in English.', action: 'Switch to English', dismiss: 'Dismiss' },
  'pt-br': { text: 'Esta página também está disponível em português.', action: 'Mudar para português', dismiss: 'Dispensar' },
  es: { text: 'Esta página también está disponible en español.', action: 'Cambiar a español', dismiss: 'Descartar' },
  de: { text: 'Diese Seite gibt es auch auf Deutsch.', action: 'Auf Deutsch wechseln', dismiss: 'Schließen' },
};

export function isLocale(v: string): v is Locale {
  return (LOCALES as readonly string[]).includes(v);
}

/** Locale-aware number formatting, e.g. 16.4 GB (en) vs 16,4 GB (pt-br/es/de). */
export function formatBytes(gb: number, locale: Locale): string {
  return `${new Intl.NumberFormat(meta[locale].bcp47, { maximumFractionDigits: 1, minimumFractionDigits: 1 }).format(gb)} GB`;
}
