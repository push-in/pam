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

## Tabelas compartilhadas e task workers

`Capability::SharedTables` e `Capability::TaskWorkers` (runtime 2.2.15+)
anunciam duas APIs PHP sobre funções nativas `pam_native_table_*` e
`pam_native_task_*`:

```php
use Pam\Shared\Table;
use Pam\Shared\Tasks;

$table = new Table(1024, name: 'app.sessions');      // sem nome: só processos filhos (fork)
$table->column('user', Table::TYPE_INT);
$table->column('token', Table::TYPE_STRING, 64);
$table->create();
$table->set('abc', ['user' => 7, 'token' => 'x']);
$table->incr('abc', 'user');                         // atômico entre workers

if (Tasks::available()) {                            // cluster com --task-workers
    $results = Tasks::resolve([$payloadA, $payloadB], 3000); // null = ainda rodando
}
```

A semântica é a de `Swoole\Table`: número fixo de linhas (arredondado para
potência de 2), colunas `int`, `float` e `string` de tamanho fixo, `set()`
devolve `false` com a tabela cheia; uma string maior que a coluna lança
`ValueError` em vez de ser truncada. Tabelas com o mesmo nome e as mesmas
colunas são a mesma memória em todos os workers do cluster.

A fronteira C/Rust é uma tabela de ponteiros de função
(`native/pam_shared.h`) que o Rust entrega ao shim antes de iniciar o PHP. O
Rust trata linhas como bytes opacos; o shim converte valores PHP antes de o
Rust pegar um lock e depois de soltá-lo, então um erro do PHP (memory_limit,
exceção) nunca deixa um lock compartilhado preso. Buffers devolvidos pelo Rust
(resultados de task, varreduras) são liberados com `free_buffer` e mensagens de
erro com `free_string`.

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
