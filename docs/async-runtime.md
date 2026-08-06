# Runtime assíncrono

## Modelo

Uma requisição HTTP vive em uma Fiber raiz. Operações Pam suspendem essa Fiber e
entregam ao Tokio um envelope tipado. O event loop aguarda sem bloquear a Zend
Engine, então outras requisições do mesmo worker podem progredir. A retomada leva o
resultado para a mesma Fiber, preservando contexto e blocos `try/finally`.

```php
use Pam\Filesystem\File;
use Pam\Http\Client;
use Pam\Net\Dns;
use Pam\Process\Command;

$addresses = Dns::resolve('example.com', timeout: 2.0);
$config = File::read(__DIR__ . '/config.json', maxBytes: 1_048_576);
$health = (new Client(timeout: 3.0))->get('https://example.com/health');
$job = Command::run([PHP_BINARY, '-r', 'echo "ready";'], timeout: 2.0);
```

DNS usa o resolver Tokio, arquivos usam a fila bloqueante do runtime, processos
recebem argv sem shell e output limitado, e sinais usam o watcher nativo. Todas as
operações respeitam o menor valor entre seu timeout e o deadline da requisição.

## Streams e backpressure

```php
use Pam\Stream\Streams;

$socket = Streams::connect('tcp://127.0.0.1:9000', timeout: 2.0);
$socket->write("ping\n");
$reply = $socket->read(timeout: 2.0);
$socket->close();
```

`Readable` e `Writable` limitam cada bloco pelo `highWaterMark`. Em respostas, a
fila Rust é limitada por `responseStreamQueueCapacity`; se o cliente estiver
lento, o envio aguarda e o generator PHP não produz chunks ilimitados.
`maxResponseBytes` limita o total e `maxResponseChunkBytes` limita cada operação;
se um stream ultrapassar o total depois de enviar headers, o transporte é encerrado
com erro para que o cliente não aceite um corpo truncado como completo.

```php
$app->get('/events', static function ($request, $response) {
    return $response->sse((static function (): Generator {
        for ($index = 1; $index <= 10; ++$index) {
            yield ['sequence' => $index];
            Pam\Async\delay(0.1);
        }
    })());
});
```

Desconexão do cliente cancela a Fiber e executa cleanup. HTTP/1.1, HTTP/2 e
HTTP/3 recebem os chunks incrementalmente.

O adaptador Laravel aplica o mesmo canal a `StreamedResponse` e
`BinaryFileResponse`, preservando downloads parciais (`Range`), `HEAD`, operações
`Pam\Async` dentro do callback e a ordem correta de `Kernel::terminate()`.

## Escopo da requisição

```php
use Pam\Runtime\RequestScope;

$scope = RequestScope::current();
$handle = $scope->manage(fopen('/tmp/app.log', 'ab'));
$scope->set('tenantId', 42);
$scope->defer(static function (): void {
    // transação, lock ou contexto externo
});
```

Cleanups executam em LIFO em sucesso, exceção, timeout e cancelamento. Não coloque
Request/Response em singletons: mantenha estado request-local no scope ou no
`FiberContext`.

## Amp e Revolt

Futures Amp/Revolt são aceitos por `Pam\Async\await()`. A ponte executa o driver
Revolt fornecido pelo pacote para preservar compatibilidade, mas não o converte em
Tokio. Use a ponte para bibliotecas Composer e as APIs nativas acima no caminho de
maior concorrência.
