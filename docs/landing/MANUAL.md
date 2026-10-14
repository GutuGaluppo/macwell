# Só você pode fazer (ou decidir)

Tudo que a landing page (`landing/`) deixa em aberto porque depende de uma decisão, de uma conta, de uma credencial ou do app rodando na sua máquina. Marque conforme avançar. Enquanto um item estiver aberto, a página mostra um `{{PLACEHOLDER}}` visível ou um estado seguro (por exemplo, o botão de download vira aviso de lista de espera).

## Bloqueia o lançamento do download

- [ ] **Notarizar o app** (Apple Developer ID, assinatura + `notarytool` + `stapler`). Sem isso o macOS bloqueia o DMG baixado. Só depois ligue `PUBLIC_DOWNLOAD_LIVE=true`.
- [ ] **Gerar o DMG** assinado e notarizado e subir no R2 com URL estável. Definir `PUBLIC_DMG_URL`.
- [ ] **Calcular o SHA-256** do DMG (`shasum -a 256 arquivo.dmg`). Definir `PUBLIC_SHA256`.
- [ ] **Definir versão, macOS mínimo e arquitetura** (Apple Silicon, Intel ou universal). `PUBLIC_VERSION`, `PUBLIC_MIN_MACOS`, `PUBLIC_ARCH`.
- [ ] **Definir o preço** (ou gratuito) e o modelo de venda/licença. `PUBLIC_PRICE` (também alimenta a FAQ "Quanto custa?" nos 4 idiomas).
- [ ] **Página de notas da versão** (URL). `PUBLIC_RELEASE_NOTES_URL`.

## Contas e infraestrutura

- [ ] **Comprar/escolher o domínio** do site e do domínio de mídia (ex.: `media.<domínio>`). Definir `SITE_URL` e `PUBLIC_MEDIA_BASE`. Hoje o site usa `https://organizemymac.example` em canonical, hreflang, sitemap e Open Graph.
- [ ] **Conta Cloudflare**: criar projeto Pages (`organizemymac`), bucket R2 para mídia e DMG, apontar o DNS, ativar HSTS/preload no domínio.
- [ ] **`wrangler login`** (ou token de API do Cloudflare) para `npm run deploy`, ou conectar o repositório no painel. Opcional: CI no GitHub Actions com `CLOUDFLARE_API_TOKEN` e `CLOUDFLARE_ACCOUNT_ID` como secrets.
- [ ] **Subir `landing/public/media/**` no R2** depois de capturar as mídias.
- [ ] **Analytics (opcional)**: se quiser, hospedar Plausible/Umami próprio e definir `PUBLIC_ANALYTICS_SRC` e `PUBLIC_ANALYTICS_DOMAIN`. Sem isso, não há analytics.
- [ ] **Teste de preview por branch** no Pages depois do primeiro deploy.

## Mídia (precisa do app rodando)

- [ ] **Instalar ffmpeg** (`brew install ffmpeg`) para `scripts/encode-video.sh`.
- [ ] **Preparar dados de demonstração** (arquivos sob `/Users/demo`, sem caminhos pessoais) e usar janela 1200x780, tema escuro.
- [ ] **Gravar os vídeos** de `MEDIA.md`. Prioridade: `hero-cleanup`, `review-dialog`, `assistant`. Depois `clutter`, `applications`, `performance`, `projects`, `menubar`, `sidebar-rail`.
- [ ] **Tirar os screenshots** (janela com sombra, 2x): `cleanup-results`, `applications-results`, `clutter-results`, `projects-results`, `review-dialog`, `operations-log`, mais três quadros dos passos: `step-analyze`, `step-review`, `step-clean` (estes três não estavam em `MEDIA.md`). Opcionais: `assistant`, `performance-results`, `security-results`, `settings`. Depois `node scripts/build-shots.mjs <pasta>`.
- [ ] **Decidir o idioma da UI gravada**: o app está em português. Recomendação: gravar uma vez em português e manter o texto da página localizado em volta. Se quiser inglês, é preciso gravar uma passada com a UI em inglês (o app precisa suportar).
- [ ] **Legendas/transcrição dos vídeos** (`.vtt`) nos 4 idiomas, depois que o roteiro final dos vídeos existir. Hoje a página tem `aria-label` e legenda curta por idioma.
- [ ] **Ligar mais vídeos à página**: além do hero, os demais vídeos (`assistant`, `clutter` etc.) ainda não têm lugar na página. Decida onde entram (ou se entram).

## Texto e jurídico

- [ ] **Política de privacidade**: redigir e publicar (e revisar com alguém habilitado). Definir `PUBLIC_PRIVACY_URL`. Hoje o link aponta para a própria seção `#privacy`.
- [ ] **E-mail de contato** e da lista de espera. `PUBLIC_CONTACT_EMAIL` (o botão "avisar por e-mail" abre um `mailto:`). Se preferir um formulário/serviço de lista de espera, isso exige uma conta e decisão de provedor (e implica tratar dados pessoais).
- [ ] **Termos de uso / licença (EULA)**, se for vender.
- [ ] **Revisão por falantes nativos** de PT-BR, ES e DE (a tradução vem do `COPY.md` e dos textos extras que escrevi: seção de privacidade, prova, download, rodapé, alt text). Conferir também o texto novo que não estava em `COPY.md`.
- [ ] **Marca e nome**: confirmar o wordmark "OrganizeMyMac" (texto) e verificar se o nome pode ser usado (marca registrada, domínio).
- [ ] **Aviso de marca da Apple**: o rodapé diz "Mac e macOS são marcas registradas da Apple Inc."; confirmar com seu jurídico se basta.

## Verificações que dependem de você

- [ ] **Lighthouse com mídia real** (vídeo no hero) em 4G simulado, nos 4 idiomas. Hoje: performance 100, CLS 0, LCP 1,6 s só com placeholders.
- [ ] **Teste em Safari, Firefox e iPhone real** (usei só o Chromium embutido).
- [ ] **Contraste e leitura do tema claro** com os screenshots reais (os escuros em fundo claro podem pedir ajuste).
- [ ] **Teste com leitor de tela** (VoiceOver) e navegação só por teclado.
- [ ] **Decidir sobre o fundo claro**: hoje o tema claro segue a preferência do sistema; se preferir só escuro, é uma linha de CSS.
- [ ] **Revisar o `.claude/launch.json`**: adicionei a entrada `landing` (porta 4321) nesse arquivo local (ele está no `.gitignore`).

## Já resolvido por mim (para você saber o que mudou de plano)

- O "Worker" para cabeçalhos virou o arquivo `dist/_headers` gerado no build (CSP, HSTS, cache), sem código de servidor. A sugestão de idioma é do navegador (lado do cliente), dispensável, sem cookie nem armazenamento.
- O projeto está em `landing/` com `package.json` próprio; nada do app Tauri foi alterado.
