# PAM Native para iOS

O PAM gera um host Xcode autocontido em `.pam-native/ios/App`. O projeto usa
Swift Package Manager localmente e não exige CocoaPods, Tuist ou XcodeGen.

## Toolchain e runtime

Em macOS, selecione o Xcode e execute:

```bash
pam mobile ios:doctor .
pam mobile ios:prepare .
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
pam mobile ios:devices .
pam mobile ios:run .
pam mobile ios:logs .
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

pam mobile ios:sign .
pam mobile ios:package .
```

O comando cria um archive de distribuição, exporta o IPA para `dist/` e grava
seu SHA-256. A autenticação do App Store Connect e os profiles devem permanecer
nos secrets/keychain do CI.
