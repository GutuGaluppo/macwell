# OrganizeMyMac

Utilitário nativo para macOS que mostra o que ocupa espaço no Mac e ajuda a remover o que não é
necessário com segurança: arquivos grandes e antigos, Downloads, duplicados, mapa de espaço e
aplicativos com os restos que deixam.

> Princípio: ser conservador ao remover dados. Tudo vai para a Lixeira, cada item mostra caminho e
> tamanho antes, e pastas do sistema são bloqueadas no código.

## Estado

| Marco | Conteúdo | Situação |
| --- | --- | --- |
| M0 | Base: Tauri + React, comandos Rust, jobs canceláveis com progresso, SQLite, permissões, camada de segurança, logs, fixtures de teste | ✅ |
| M1 | Armazenamento: visão geral dos discos, arquivos grandes e antigos, Downloads, Lixeira, Mostrar no Finder, Visualização Rápida, mover para a Lixeira com revisão | ✅ |
| M2 | Duplicados: grupos por tamanho, hash de trechos, BLAKE3 completo, seleção automática que sempre mantém uma cópia, hard links reconhecidos | ✅ |
| M3 | Mapa de espaço: treemap proporcional (squarified), lista hierárquica, navegação por pastas, trilha de navegação e seleção | ✅ |
| M4 | Aplicativos: lista com tamanho, versão e último uso; desinstalação com prévia completa dos arquivos em ~/Library por nível de confiança; restos de apps desinstalados | ✅ |
| M5 | Saúde do Mac e barra de menus: CPU, memória, swap, disco, bateria, tempo ligado, apps abertos e os que mais usam memória | ✅ |
| M6 | Imagens parecidas: helper em Swift (Vision), pré-filtro perceptivo, feature prints, agrupamento e integração com o Fotos | ✅ |
| M7 | Cuidado inteligente: orquestra as análises, recomendações determinísticas, tela única de revisão e plano de limpeza seguro | ✅ |
| M8 | Limpeza avançada: banco de regras (caches, registros, ferramentas de desenvolvimento, navegadores, anexos do Mail) e regras personalizadas | ✅ |
| M9 | Helper privilegiado de manutenção (DNS, Spotlight, snapshots do Time Machine): **só o design e a revisão de segurança** ([docs/M9-PRIVILEGED-HELPER-REVIEW.md](docs/M9-PRIVILEGED-HELPER-REVIEW.md)); o plano exige essa revisão antes de qualquer código | 📝 |
| M10 | Nuvem e atualizações: uso local de iCloud Drive, Google Drive, Dropbox e OneDrive, remover download do iCloud, detecção de versões (App Store e Sparkle) | ✅ |
| M11 | Trilha de pesquisa em segurança: **auditoria** de itens de início e de assinaturas de apps (implementada, experimental); YARA, serviço de assinaturas e Endpoint Security documentados como pesquisa ([docs/M11-SECURITY-RESEARCH.md](docs/M11-SECURITY-RESEARCH.md)) | ✅ / 📝 |
| M12 | Projetos (Developer Care): repositórios git, artefatos regeneráveis, verificação de segredos e snapshot WIP em branch local ([docs/M12-DEVELOPER-CARE.md](docs/M12-DEVELOPER-CARE.md)) | ✅ |
| M13 | Backup Guard: estado do Time Machine e do iCloud, portão antes de limpezas em lote, exclusões do Time Machine ([docs/M13-BACKUP-GUARD.md](docs/M13-BACKUP-GUARD.md)) | ✅ |
| M14 | Quarentena e relatório: pasta datada no mesmo volume com log CSV, restaurar, relatório em Markdown/CSV e análise do Homebrew, só leitura ([docs/M14-QUARANTINE.md](docs/M14-QUARANTINE.md)) | ✅ |

M0–M4 formam o MVP 1.0 recomendado no plano. O plano completo está em [docs/IMPLEMENTATION.md](docs/IMPLEMENTATION.md).

## Funcionalidades

- **Visão geral:** espaço livre e usado de cada disco, atalhos e últimas análises.
- **Cuidado inteligente:** uma análise que junta Lixeira, Downloads, duplicados acima de 10 MB, arquivos grandes esquecidos, apps sem uso e restos de apps (≈15 s no Mac de desenvolvimento), com recomendações por regras fixas (sem IA) e uma revisão por seções. Só vem marcado o que é seguro; a Lixeira nunca vem marcada e, se escolhida, é esvaziada antes e apenas com os itens revisados.
- **Scanner:** o que ocupa espaço em uma pasta ou no disco inteiro, com totais lógico e em disco, pastas e maiores arquivos.
- **Mapa de espaço:** retângulos com área proporcional ao espaço, dois níveis por vez; clique numa pasta para entrar, trilha de navegação para voltar, lista hierárquica ao lado e seleção de pastas ou arquivos para a Lixeira. Usa a análise do Scanner, sem ler o disco de novo.
- **Grandes e antigos:** filtros por tamanho (100 MB, 500 MB, 1 GB), tempo sem modificação e tipo; arquivos dentro de apps e bibliotecas não são separados.
- **Downloads:** instaladores antigos (pré-selecionados), arquivos grandes, compactados (os já extraídos aparecem como seguros), instaladores recentes, capturas de tela e arquivos antigos.
- **Duplicados:** agrupa por tamanho, compara trechos do início, meio e fim e só então calcula o BLAKE3 completo (em ~/Projects: 8.307 candidatos, 1.616 lidos por inteiro, 1,8 GB/s). A seleção automática mantém a cópia fora de Downloads e da Lixeira, sem "cópia" no nome e mais antiga; o app recusa remover todas as cópias de um grupo e pula as que mudaram desde a análise.
- **Imagens parecidas:** um helper em Swift (sidecar do Tauri) gera miniaturas, calcula um hash perceptivo para descartar o que não tem par e só então roda o Vision (`VNGenerateImageFeaturePrintRequest`) nos candidatos. Duas sensibilidades, calibradas com cópias redimensionadas, recomprimidas e recortadas. Mantém a imagem de maior resolução. Também funciona na biblioteca do Fotos pelo PhotoKit: nunca mexe nos arquivos internos da biblioteca, não baixa originais do iCloud e apaga só pelo próprio Fotos (com a confirmação dele e Apagados Recentemente).
- **Limpeza avançada:** regras em `rules/cleanup/builtin.json` com local, risco, apps que precisam estar fechados e seleção padrão: caches e registros de apps, DerivedData e suporte a dispositivos do Xcode, caches do simulador, npm, Cargo e Gradle, cache/histórico/cookies do Chrome, cache/cookies/formulários do Firefox (favoritos e histórico em `places.sqlite` nunca são tocados) e cópias de anexos do Mail. Caches de apps abertos ficam bloqueados, inclusive a pasta do fabricante (ex.: `Caches/Google` com o Chrome aberto); caches do macOS nunca vêm marcados. Regras personalizadas para pastas dentro da pasta pessoal (Documentos, Mesa, Fotos e afins não podem ser usados).
- **Lixeira:** quanto ocupa e o que tem; esvaziar pede confirmação. Sem Acesso Total ao Disco, dá para pedir ao Finder.
- **Aplicativos:** apps de /Applications e ~/Applications com tamanho, versão e último uso (Spotlight); filtros "sem uso há 6 meses" e App Store. Apps do macOS são protegidos; apps abertos precisam ser encerrados antes.
- **Desinstalar:** o app e os arquivos dele em ~/Library, com o motivo de cada item e o total por local. Identificador exato do app e pastas conhecidas (rules/apps) são *seguros* e já vêm marcados; pasta do fabricante ou com nome parecido pede *revisão*; contêiner compartilhado é *risco*. O servidor refaz o plano e só aceita caminhos que fazem parte dele.
- **Restos de apps:** arquivos em ~/Library com o identificador de um app que não está em nenhum disco indexado pelo Spotlight. Identificadores da Apple são ignorados; se o fabricante ainda tem apps instalados, o item fica para revisão.
- **Desempenho:** CPU com histórico, memória e swap, disco, bateria (pmset), tempo ligado e processos separados em aplicativos (helpers somados ao app), segundo plano e sistema. "Encerrar" só aparece para apps comuns seus e equivale a escolher Encerrar no menu do app (nunca `kill -9`). Atualiza a cada 2 s e para quando a janela está escondida.
- **Barra de menus:** CPU e/ou memória no título; no menu, disco, bateria, tempo ligado e os 5 apps que mais usam memória. Métricas a cada 5 s e processos a cada 30 s, em prioridade baixa (≈0,4% de um núcleo no build de desenvolvimento). Com ela ligada, fechar a janela mantém o app na barra.
- **Nuvem:** quanto de cada pasta sincronizada está baixado, quantos arquivos estão só na nuvem e os maiores baixados. No iCloud Drive, "Remover download" usa a API pública (`evictUbiquitousItem`): o arquivo continua no iCloud. Para Google Drive, Dropbox e OneDrive o app mostra os números e explica a opção "somente on-line" de cada um (não há API pública para fazer isso por eles).
- **Atualizações:** versão instalada de cada app; apps da App Store e com atualizador Sparkle (feed HTTPS declarado no app) são consultados só quando você pede. Betas e canais de teste são ignorados. O app nunca baixa nem instala: abre o app ou a App Store.
- **Auditoria de segurança (experimental):** LaunchAgents e LaunchDaemons com o programa, a assinatura e observações neutras (sem assinatura, rótulo `com.apple.` sem assinatura da Apple, pasta temporária ou oculta, script embutido, programa que não existe mais), e a assinatura e a avaliação do Gatekeeper de cada app. Não é antivírus e não altera nada: leva ao Finder ou aos Itens de Início dos Ajustes do Sistema.
- Em qualquer lista: Visualização Rápida, Mostrar no Finder e Ignorar. Antes de mover, uma revisão mostra cada caminho, o tamanho e o total.

## Stack

- **UI:** Tauri 2, React 19, TypeScript, Vite, Tailwind CSS 4, Zustand, TanStack Query, Lucide
- **Núcleo:** Rust, rayon, BLAKE3, plist, SQLite (`rusqlite`), `trash`
- A UI nunca executa comandos nem acessa o disco: tudo passa por comandos Rust tipados
  ([ARCHITECTURE.md](docs/ARCHITECTURE.md)).

## Segurança e permissões

- [docs/SAFETY.md](docs/SAFETY.md): regras aplicadas antes de cada remoção e os testes de cada uma.
- [docs/PERMISSIONS.md](docs/PERMISSIONS.md): Acesso Total ao Disco (opcional) e como o app evita
  travar em pedidos de permissão.

## Desenvolvimento

Requisitos: macOS 13+, Node 20+, Rust estável, Xcode (o `build.rs` compila o helper em Swift em `native/macos-helper`).

```bash
npm install
npm run tauri dev
```

Testes do núcleo (unidade e integração com fixtures sintéticas) e da interface:

```bash
cd src-tauri && cargo test
```

```bash
npm test
```

Benchmark de leitura (não altera nada):

```bash
cd src-tauri && cargo run --release --example bench_scan -- ~/
```

Prévia da interface no navegador, com dados fictícios: `npm run dev` e abra http://localhost:1480.

Build de release (app de 9,7 MB com o helper em Swift):

```bash
npm run tauri build -- --bundles app
```

Distribuir para outras pessoas exige assinatura Developer ID, Hardened Runtime (já ligado em
`tauri.conf.json`) e notarização; o build local sai com assinatura ad hoc.

## Privacidade

Sem telemetria. O app só acessa a rede quando você pede para verificar atualizações: consulta a App Store (identificador de cada app da App Store) e o endereço de atualização HTTPS que cada app declara. Histórico de análises, registro de operações e estatísticas ficam só no Mac, em
`~/Library/Application Support/dev.galuppo.OrganizeMyMac`.
