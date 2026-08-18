# PAM Native para Android

O Android é a primeira plataforma certificada do PAM Native. Todos os comandos
partem da raiz do projeto, onde está o `pam-native.json`.

Se você ainda não possui o PAM, comece pela
[instalação única do CLI](getting-started.md#install). Depois crie o projeto com
`pam init meu-app --template mobile`. O CLI detecta Android pelo contexto do
projeto; os comandos `pam mobile ...` são aliases explícitos para CI e automação
avançada.

## Preparar o ambiente

Instale Java 17 ou superior e o Android SDK. O PAM instala seus runtimes PHP e
engines nativos verificados; o Android SDK fornece NDK `27.1.12297006`, CMake
`3.22.1`, platform-tools e API 36. Depois execute:

```bash
pam doctor --fix
pam doctor
pam build
```

`pam doctor --fix` baixa o bundle atestado da release, verifica SHA-256 e
instala os runtimes PHP 8.4/8.5 e engines para `arm64-v8a` e `x86_64`. Em um
projeto com `pam-registry.json`, URL, hash, compatibilidade com PAM e protocolo
Native vêm do catálogo assinado; o piso antirrollback só avança após instalação
completa. A proveniência em `.pam-native/android-runtime.artifact.json` força
reinstalação quando raiz, catálogo, versão ou hash mudam. Rust só é
necessário para contribuir com o engine; uma aplicação comum usa os binários
pré-compilados. O doctor falha se qualquer exigência restante estiver ausente.

## Desenvolver

```bash
pam devices
pam run
pam dev
pam logs
pam devtools
pam diagnostics
pam mobile screenshot . --output artifacts/screenshots/home.png
```

`run` detecta a ABI do aparelho conectado. `dev` mantém o runtime em modo debug
e entrega hot reload. Para escolher explicitamente a ABI use `--abi
arm64-v8a` ou `--abi x86_64`.

`diagnostics` captura o snapshot vivo e redigido do host Android em JSON. A
timeline omite mensagens e rótulos da aplicação, é limitada a oito eventos e o
arquivo privado intermediário é apagado após a leitura.

`screenshot` captura a tela do aparelho via `adb exec-out`, valida assinatura e
header PNG antes de gravar e restringe o destino ao projeto. Ele recusa substituir
uma golden existente; use `--force` somente quando a atualização visual for
intencional. O caminho padrão é `artifacts/screenshots/android.png`.

## Gerar uma release assinada

Antes de assinar, audite toda autoridade nativa agregada pelo aplicativo e por
plugins Composer:

```bash
pam mobile audit . --deny-high
pam mobile audit . --deny-high --json > artifacts/mobile-release-audit.json
```

O relatório ordena permissões Android, deep links, share targets, repositórios e
dependências nativas por severidade. Dependências dinâmicas, acesso amplo a
arquivos, instalação de pacotes e enumeração global de apps bloqueiam a release
por padrão. `--deny-high` também transforma câmera, microfone, localização e
outras autoridades sensíveis em falha de CI.

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
