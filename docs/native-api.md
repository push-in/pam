# Native API

Pam separa a API PHP pública da fronteira C/Rust. `native/pam.h` é o contrato
do shim e começa na ABI `1`. Rust verifica a ABI antes de inicializar a Embed SAPI;
uma divergência encerra o boot com erro, em vez de executar símbolos incompatíveis.

## Descoberta no PHP

```php
use Pam\Native\Api;
use Pam\Native\Capability;

if (Api::abiVersion() !== Api::ABI_VERSION) {
    throw new RuntimeException('ABI nativa incompatível');
}

if (Api::supports(Capability::HttpStreaming)) {
    // integração opcional
}
```

Capabilities são enums inteiros estáveis: timer, readiness de stream, DNS,
filesystem, processo, sinal, streaming HTTP, WebSocket e HTTP/3. Adições futuras
recebem novos valores; significados existentes não são reutilizados.

## Regras de evolução

- incrementar a ABI ao mudar assinatura, ownership ou representação de dados;
- manter códigos e enums numéricos existentes;
- validar tamanho, UTF-8/JSON, status e headers em ambos os lados da fronteira;
- nunca chamar Zend fora da thread que inicializou o worker;
- documentar quem aloca e libera cada ponteiro;
- incluir teste de boot incompatível e da capability nova antes do release.

As funções C exportadas são deliberadamente pequenas: lifecycle da Embed SAPI,
execução de arquivo, begin/resume/cancel de dispatch HTTP, eventos WebSocket,
configuração/diagnóstico e extração segura de file descriptor de `php_stream`.
Extensões de terceiros devem preferir a API PHP; consumo direto de `pam.h` exige
fixar e verificar `PAM_NATIVE_ABI_VERSION`.
