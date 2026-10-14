import { defineConfig } from 'astro/config';
import sitemap from '@astrojs/sitemap';
import tailwindcss from '@tailwindcss/vite';

const site = process.env.SITE_URL || 'https://organizemymac.example';

export default defineConfig({
  site,
  trailingSlash: 'always',
  build: { inlineStylesheets: 'never' },
  integrations: [
    sitemap({
      i18n: {
        defaultLocale: 'en',
        locales: { en: 'en', 'pt-br': 'pt-BR', es: 'es', de: 'de' },
      },
    }),
  ],
  vite: {
    plugins: [tailwindcss()],
    // Never inline scripts: the CSP in dist/_headers allows script-src 'self' only.
    build: { assetsInlineLimit: 0 },
  },
});
