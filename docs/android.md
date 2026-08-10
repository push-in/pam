# PAM Native para Android

O Android é a primeira plataforma certificada do PAM Native. Todos os comandos
partem da raiz do projeto, onde está o `pam-native.json`.

Se você ainda não possui o PAM, comece pela
[instalação única do CLI](getting-started.md#install). Depois crie o projeto com
`pam init meu-app --template mobile`. O CLI detecta Android pelo contexto do
projeto; os comandos `pam mobile ...` são aliases explícitos para CI e automação
avançada.

## Preparar o ambiente

Instale Java 17 ou superior, Rust, Android SDK 36, NDK `27.1.12297006`, CMake
`3.22.1` e platform-tools. Depois execute:

```bash
pam doctor --fix
pam doctor
pam build
```

O doctor valida também os targets Rust e os runtimes PHP verificados para
`arm64-v8a` e `x86_64`. Ele falha se uma exigência de build estiver ausente.

## Desenvolver

```bash
pam devices
pam run
pam dev
pam logs
pam devtools
```

`run` detecta a ABI do aparelho conectado. `dev` mantém o runtime em modo debug
e entrega hot reload. Para escolher explicitamente a ABI use `--abi
arm64-v8a` ou `--abi x86_64`.

## Gerar uma release assinada

PAM lê as credenciais somente do ambiente:

```bash
export PAM_ANDROID_KEYSTORE=/caminho/release.jks
export PAM_ANDROID_KEY_ALIAS=release
export PAM_ANDROID_KEYSTORE_PASSWORD='...'
export PAM_ANDROID_KEY_PASSWORD='...'

pam sign
pam package
```

`package` recusa uma release sem assinatura. A pasta `dist/` recebe:

- um Android App Bundle `.aab` para a Play Store;
- um APK universal `.apk` para distribuição e QA;
- um arquivo `.sha256` para cada binário;
- `android-release.json` com application ID, version code, version name,
  runtime e nomes dos artefatos.

Senhas e keystores nunca são gravados no projeto. Guarde-os como secrets do CI.

## Certificação

O CI compila e testa o renderer, plugin API, lint, unit tests e instrumented
tests nas APIs Android 26 e 36. O workflow de ecossistema compila em conjunto
todos os plugins oficiais para detectar conflitos de autolink, manifestos e
dependências Gradle.
