# PAM Native para iOS

O PAM gera um host Xcode autocontido em `.pam-native/ios/App`. O projeto usa
Swift Package Manager localmente e não exige CocoaPods, Tuist ou XcodeGen.

Se você ainda não possui o PAM, comece pela
[instalação única do CLI](getting-started.md#install). Depois crie o projeto com
`pam init meu-app --template mobile --platform ios`. Dentro do projeto, os
comandos curtos abaixo selecionam iOS pelo manifesto e pelo ambiente. Os aliases
`pam mobile ios:*` continuam disponíveis para CI e automação explícita.

## Toolchain e runtime

Em macOS, selecione o Xcode e execute:

```bash
pam doctor
pam build
```

O doctor valida Xcode, command-line tools, os targets Rust Apple e os
XCFrameworks verificados do PHP Embed e do engine PAM. Para construir esses
artefatos a partir das fontes verificadas:

```bash
runtime-builder/ios/build.sh --php 8.5 all
```

O builder produz slices para device arm64 e simuladores arm64/x86_64.

## Simulador

Com um simulador inicializado:

```bash
pam devices
pam run
pam dev
pam logs
pam devtools
pam diagnostics
pam mobile ios:screenshot . --output artifacts/screenshots/home-ios.png
```

`ios:run` prepara o host, compila, instala e inicia o bundle ID declarado em
`pam-native.json`.

`dev` faz a compilação e instalação inicial uma vez e abre o servidor local de
hot reload em `127.0.0.1:39100`. Cada mudança gera um bundle `PNA1` determinístico
de no máximo 16 MiB; o host debug valida a resposta durante o download, recusa
redirects, ativa a nova árvore PHP de forma transacional em
`Library/Caches/pam/dev` e preserva a versão ativa se a atualização falhar. O
cache do simulador mantém apenas a versão ativa. No projeto, o próximo `dev`
remove tanto `DerivedData` quanto o bundle temporário anterior antes de compilar,
evitando o acúmulo de builds antigos.

`devtools` alterna a overlay UIKit no host debug gerado, com FPS, custos de
decode/mount, p95 do engine, commits e timeline de capacidades.

`diagnostics` captura um snapshot schema 1 do simulador em execução, com
métricas agregadas, timeline limitada e sem mensagens ou labels da aplicação.
O bloco `hotReload` inclui a janela limitada a 64 amostras, p95 do aceite da
versão até o primeiro frame, falhas e o orçamento de 1.000 ms. Ele pode ser
validado offline com `pam-native/scripts/check-hot-reload-evidence.php` usando o
mesmo contrato aceito para Android e iOS.
O host debug grava em seu cache privado fora da main thread; a CLI impõe limite
de 64 KiB e remove o arquivo imediatamente após a leitura. A forma explícita é
`pam mobile ios:diagnostics .`.

`ios:screenshot` captura o simulador inicializado, valida o PNG e remove o arquivo
temporário de `.pam-native` antes de concluir. O destino precisa permanecer dentro
do projeto e não é sobrescrito sem `--force`; por padrão, a captura fica em
`artifacts/screenshots/ios.png`.

## Assinatura e IPA

Execute o mesmo gate de autoridade antes do archive:

```bash
pam mobile audit . --deny-high
pam mobile audit . --deny-high --json > artifacts/mobile-release-audit.json
```

No iOS, o relatório cobre privacy usage descriptions, tracking, entitlements,
extensões e requisitos Swift Package que possam mudar sem alteração no descritor.
O JSON usa contrato estável: `schemaVersion: 1`, `surfaceCode: 2`, resultado
inteiro `1` (pass) ou `2` (fail), e severidades sequenciais de `1` a `4`.

PAM não grava certificados, credenciais ou provisioning profiles no projeto.
Defina o time Apple e forneça um `ExportOptions.plist` controlado pelo ambiente
de release:

```bash
export PAM_IOS_DEVELOPMENT_TEAM=ABCDE12345
export PAM_IOS_EXPORT_OPTIONS_PLIST=/secure/ExportOptions.plist

pam sign
pam package
```

O comando cria um archive de distribuição, exporta o IPA para `dist/` e grava
seu SHA-256. A autenticação do App Store Connect e os profiles devem permanecer
nos secrets/keychain do CI.

## Certificação

O workflow de iOS gera projetos do zero, resolve os plugins por Composer após
preflight, compila os targets e extensões, instala o aplicativo e confirma que o
processo PHP permanece executando em um simulador real. A matriz cobre PHP 8.4
com iOS 15 e PHP 8.5 com iOS 18; Share Extension, Health, Media, Widgets, App
Intents e Live Activities são validados nos níveis mínimos compatíveis.
