// Single place for every value that is not decided yet. Empty env var = visible {{PLACEHOLDER}}.
const env = import.meta.env;
const ph = (value: string | undefined, name: string) => (value && value.trim() ? value.trim() : `{{${name}}}`);

export const SITE_URL: string = (import.meta.env.SITE ?? 'https://organizemymac.example').replace(/\/$/, '');
export const MEDIA_BASE: string = (env.PUBLIC_MEDIA_BASE || '/media').replace(/\/$/, '');

/** The download button stays a waitlist link until this is "true". Do not flip before notarization. */
export const DOWNLOAD_LIVE: boolean = env.PUBLIC_DOWNLOAD_LIVE === 'true';

export const values = {
  PRICE: ph(env.PUBLIC_PRICE, 'PRICE'),
  VERSION: ph(env.PUBLIC_VERSION, 'VERSION'),
  DMG_URL: ph(env.PUBLIC_DMG_URL, 'DMG_URL'),
  MIN_MACOS: ph(env.PUBLIC_MIN_MACOS, 'MIN_MACOS'),
  ARCH: ph(env.PUBLIC_ARCH, 'ARCH'),
  SHA256: ph(env.PUBLIC_SHA256, 'SHA256'),
  RELEASE_NOTES_URL: ph(env.PUBLIC_RELEASE_NOTES_URL, 'RELEASE_NOTES_URL'),
};

export const PRIVACY_URL: string = env.PUBLIC_PRIVACY_URL || '#privacy';
export const CONTACT_EMAIL: string = env.PUBLIC_CONTACT_EMAIL || '';
export const ANALYTICS = {
  src: env.PUBLIC_ANALYTICS_SRC || '',
  domain: env.PUBLIC_ANALYTICS_DOMAIN || '',
};

/** Replaces {{NAME}} tokens in copy with config values. */
export function fill(text: string): string {
  return text.replace(/\{\{(\w+)\}\}/g, (m, k: string) => (k in values ? values[k as keyof typeof values] : m));
}
