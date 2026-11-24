# Retentativas, circuit breaker, transações e prazos

Primitivas para chamadas que falham: `Nimbus\Resilience` (retentativa com backoff e circuit breaker), `Nimbus\Transactions` (transação por operação ou por requisição) e `Nimbus\Runtime\Deadline` (prazo cooperativo da requisição).

## Retentativa

`Nimbus\Resilience\RetryPolicy` repete uma operação que lança exceção, com espera exponencial a partir de `initialDelayMilliseconds` (10, 20, 40 ms... com o padrão):

```php
use Nimbus\Resilience\RetryPolicy;

$retry = new RetryPolicy(
    attempts: 3,
    initialDelayMilliseconds: 50,
    when: static fn (\Throwable $error): bool => $error instanceof \PDOException,
);

$balance = $retry->run(static fn (int $attempt): int => $ledger->balance($accountId));
```

`run()` passa o número da tentativa (a partir de 1) e devolve o resultado da primeira que der certo. Quando as tentativas acabam, ou `when` devolve `false`, a última exceção sobe. Sem `when`, toda exceção é repetida.

A espera usa `usleep()`, que bloqueia o worker inteiro, e não só a Fiber da requisição. Use esperas curtas nesse caminho. Para HTTP, prefira o `retry()` do [cliente HTTP](./http-client.md#novas-tentativas), que espera sem bloquear dentro do PAM; para jobs, a `RetryPolicy` da fila ([Filas](./filas.md)) reagenda o job em vez de esperar.

## Circuit breaker

`Nimbus\Resilience\CircuitBreaker` para de chamar um serviço depois de `failureThreshold` falhas seguidas e só tenta de novo depois de `cooldownSeconds`:

```php
use Nimbus\Resilience\CircuitBreaker;
use Nimbus\Resilience\CircuitOpenException;

$app->container()->singleton(CircuitBreaker::class, static fn () => new CircuitBreaker(failureThreshold: 5, cooldownSeconds: 30));

final readonly class ShippingQuotes
{
    public function __construct(private CircuitBreaker $breaker, private CarrierApi $carrier) {}

    public function quote(string $zip): ?int
    {
        try {
            return $this->breaker->call(fn (): int => $this->carrier->priceInCents($zip));
        } catch (CircuitOpenException) {
            return null;   // serviço fora: responde sem frete em vez de esperar o timeout
        }
    }
}
```

| Estado (`CircuitState`) | Comportamento |
| --- | --- |
| `Closed = 1` | chama a operação; um sucesso zera o contador de falhas |
| `Open = 2` | lança `CircuitOpenException` sem chamar a operação |
| `HalfOpen = 3` | depois do `cooldownSeconds`: a próxima chamada passa; sucesso fecha, falha reabre |

`state()` devolve o estado atual. As exceções da operação sobem normalmente; o breaker só conta.

O estado fica na memória do objeto. Registrado como `singleton`, vale para o worker inteiro, entre requisições; cada worker e cada servidor tem o seu circuito. Use um breaker por serviço externo.

## Transações

`Nimbus\Transactions\TransactionManager` é o contrato `transaction(callable $operation): mixed`. Com o Arcano, o `ArcanoServiceProvider` registra o `ArcanoManager` como implementação ([Banco](./banco.md#transacoes)):

```php
use Nimbus\Transactions\TransactionManager;

final readonly class PlaceOrder
{
    public function __construct(private TransactionManager $transactions, private ArcanoManager $arcano) {}

    public function __invoke(Order $order, Payment $payment): Order
    {
        return $this->transactions->transaction(function () use ($order, $payment): Order {
            $this->arcano->entityManager()->persist($order)->persist($payment)->run();
            return $order;
        });
    }
}
```

Uma exceção dentro do callback desfaz a transação e sobe. O valor devolvido pelo callback é o retorno de `transaction()`. Depender do contrato, e não do `ArcanoManager`, deixa o serviço testável com uma implementação falsa.

### Uma transação por requisição

`Nimbus\Middleware\TransactionalMiddleware` envolve o handler inteiro numa transação: uma exceção não tratada desfaz tudo o que a requisição gravou.

```php
use Nimbus\Middleware\TransactionalMiddleware;
use Nimbus\Transactions\TransactionManager;

$transactional = new TransactionalMiddleware($app->container()->get(TransactionManager::class));

$router->post('api/checkout', [CheckoutController::class, 'store'])->middleware($transactional);
```

Uma resposta de erro devolvida (sem exceção), como um `422` montado à mão, não desfaz nada: só exceções desfazem. Não dispare jobs, e-mails ou chamadas HTTP que dependem do commit dentro da transação; eles podem sair antes de um rollback.

## Prazo da requisição

`Nimbus\Middleware\DeadlineMiddleware` dá a cada requisição um `Nimbus\Runtime\Deadline`. Se o handler termina depois do prazo, a resposta vira `504` em problem+json (`ProblemCode::Timeout`):

```php
use Nimbus\Middleware\DeadlineMiddleware;

$app->middleware(new DeadlineMiddleware($app->container(), seconds: 10));
```

O prazo é cooperativo: o middleware não interrompe código em execução. Em laços longos, consulte o prazo e pare cedo:

```php
use Nimbus\Runtime\Deadline;

public function export(Request $request, Response $response, Deadline $deadline): Response
{
    foreach ($this->reports->pages() as $page) {
        $deadline->throwIfExpired();   // HttpException 504
        $this->writer->append($page);
    }
    return $response->json(['status' => 'ok']);
}
```

`expired()` devolve `bool`; `throwIfExpired()` lança `HttpException` 504. O `Deadline` fica no escopo da requisição, então só pode ser injetado em rotas que passam pelo `DeadlineMiddleware`. Combine o prazo da requisição com o `timeout()` do cliente HTTP e o timeout do banco, abaixo dele, para que as chamadas externas falhem antes do prazo.
