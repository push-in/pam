# Logs e observabilidade

## Logs

Dentro do PAM, `Psr\Log\LoggerInterface` resolve para o log estruturado do runtime. Injete onde precisar:

```php
use Psr\Log\LoggerInterface;

final readonly class CheckoutController
{
    public function __construct(private LoggerInterface $log) {}

    public function store(CheckoutInput $input, Response $response): Response
    {
        $this->log->info('checkout.created', ['order_id' => $order->id, 'total_in_cents' => $order->total]);
        // ...
    }
}
```

Exceções não tratadas são registradas no log com a mensagem; o cliente recebe só o problem+json genérico. Não coloque segredos nem dados pessoais no contexto do log.

Para ler os logs de um app gerenciado pelo PAM:

```bash
pam logs              # consulta ou acompanha os logs
pam monit             # saúde e capacidade dos processos
pam top               # métricas ao vivo
```

O servidor PAM também expõe métricas Prometheus, trace context W3C e traces OTLP/HTTP; veja a [documentação de observabilidade do PAM](https://github.com/push-in/pam/blob/main/docs/observability.md).

## Observadores do ciclo da requisição

Um `Nimbus\Lifecycle\RequestLifecycleObserver` recebe `starting($request)` e `finished($request, $response, $failure)` em volta de todo o pipeline. Serve para profilers, traces e diagnóstico de vazamento:

```php
use Nimbus\Lifecycle\RequestLifecycleObserver;
use Pam\Http\Request;
use Pam\Http\Response;

final class SlowRequestLog implements RequestLifecycleObserver
{
    /** @var \WeakMap<Request, float> */
    private \WeakMap $started;

    public function __construct(private LoggerInterface $log)
    {
        $this->started = new \WeakMap();
    }

    public function starting(Request $request): void
    {
        $this->started[$request] = hrtime(true) / 1e6;
    }

    public function finished(Request $request, Response $response, ?\Throwable $failure): void
    {
        $elapsed = hrtime(true) / 1e6 - ($this->started[$request] ?? 0.0);
        if ($elapsed > 500) {
            $this->log->warning('slow request', ['path' => $request->path, 'ms' => (int) $elapsed]);
        }
    }
}

$app->observe(new SlowRequestLog($logger));
```

Os observadores começam na ordem de registro e terminam na ordem inversa, antes do escopo da requisição ser destruído. Uma falha num observador vai para o log e não impede os outros nem a limpeza do container.

## Profiler

`Nimbus\Profiler\MemoryProfiler` é um coletor limitado (até 500 requisições por padrão). Ligue só em desenvolvimento ou testes:

```php
use Nimbus\Profiler\MemoryProfiler;
use Nimbus\Profiler\ProfilerMode;

$profiler = new MemoryProfiler(ProfilerMode::Development);
$app->observe($profiler);

$profiler->recent(20);          // últimas requisições
$profiler->find($debugToken);   // pelo cabeçalho X-Debug-Token
```

Guarda só método, caminho, status, duração, variação de memória e classe da falha; corpos e cabeçalhos nunca são retidos. O modo padrão, `Disabled`, não emite nada.

## Health checks

```php
use Nimbus\Database\DatabaseHealthCheck;
use Nimbus\Health\HealthRegistry;

$health = (new HealthRegistry())
    ->add('database', $app->container()->get(DatabaseHealthCheck::class));

$router->get('health', static function (Request $request, Response $response) use ($health): Response {
    $report = $health->run();   // ['state' => 1|2|3, 'checks' => ['database' => ['state' => ..., 'details' => [...]]]]
    return $response->json($report, $report['state'] === 3 ? 503 : 200);
});
```

`HealthState`: `Healthy = 1`, `Degraded = 2`, `Unhealthy = 3`; o estado geral é o pior entre as verificações.

Uma verificação própria implementa `Nimbus\Health\HealthCheck` (`check(): HealthResult`). O `pam deploy` usa a URL de health (`--health-check-url`) para só trocar de release quando a nova responde.
