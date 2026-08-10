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
pam logs
```

`ios:run` prepara o host, compila, instala e inicia o bundle ID declarado em
`pam-native.json`.

## Assinatura e IPA

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
