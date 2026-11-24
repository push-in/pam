# Redis, cache de resposta, rate limit e idempotência

## Conexão

`Nimbus\Redis\RedisPool` usa o cliente Redis não bloqueante do PAM. Cada comando pega uma conexão ociosa do pool, então Fibers de requisições diferentes nunca dividem um socket no meio de um comando. No máximo `maxIdle` conexões (16 por padrão) ficam abertas entre requisições.

```php
use Nimbus\Redis\RedisPool;

$redis = RedisPool::fromEnvironment();   // REDIS_HOST, REDIS_PORT, REDIS_PASSWORD, REDIS_DB
$app->container()->instance(RedisPool::class, $redis);

$redis->command(['SET', 'greeting', 'olá', 'EX', '60']);
$value = $redis->command(['GET', 'greeting']);
```

`RedisPool` implementa `Nimbus\Redis\Connection` (`command(array $arguments): mixed`), que é o tipo aceito pelos stores Redis abaixo.

Stores Redis disponíveis:

| Classe | Para |
| --- | --- |
| `RedisCacheStore` | cache da aplicação com tags ([Cache](./cache.md)) |
| `RedisRateLimitStore` | `RateLimitMiddleware` |
| `RedisResponseCacheStore` | `ResponseCacheMiddleware` |
| `RedisIdempotencyStore` | `IdempotencyMiddleware` |
| `RedisSessionStore` | sessões web ([Sessão](./sessao.md)) |
| `RedisTokenRevocationStore` | revogação de tokens ([Autenticação](./autenticacao.md)) |
| `RedisOAuthStateStore` | estado do login social |
| `Nimbus\Jobs\RedisJobQueue`, `Nimbus\Scheduling\RedisSchedulerLock` | [Filas e agendador](./filas.md) |
| `Nimbus\Broadcasting\RedisBroadcaster` | broadcasting por pub/sub ([Eventos](./eventos.md#drivers)) |

Os stores em memória (`MemoryRateLimitStore`, `MemoryResponseCacheStore`, `MemoryIdempotencyStore`) têm tamanho limitado e valem só dentro de um worker: servem para desenvolvimento e testes. Com mais de um worker ou servidor, use Redis.

## Rate limit

### Limitadores nomeados (`throttle`)

Para portar o `RateLimiter::for()` e o middleware `throttle` do Laravel: janelas fixas por segundo, minuto, hora ou dia, chave por `by()`, e os cabeçalhos `X-RateLimit-Limit`, `X-RateLimit-Remaining`, e no `429` também `Retry-After` e `X-RateLimit-Reset`.

```php
use Nimbus\RateLimit\Limit;
use Pam\Http\Request;

$app->rateLimiter()
    ->for('buck', fn (Request $request) => Limit::perMinute(120)->by($request->getHeader('x-account') ?? ''))
    ->for('buck-ask', fn (Request $request) => [
        Limit::perMinute(10)->by($request->getHeader('x-account') ?? ''),
        Limit::perDay(500)->by($request->getHeader('x-account') ?? ''),
    ]);

$router->post('ask', AskController::class)->rateLimit('buck-ask');   // throttle:buck-ask
$router->post('login', LoginController::class)->throttle(3, 1);      // throttle:3,1
$router->middleware('throttle:buck')->group(function ($router) { ... });
```

- `throttle($tentativas, $minutos)` conta por principal autenticado ou IP do cliente, separado por rota.
- `Limit::none()` libera a requisição; `->response(fn (Request $r, Response $res, array $headers) => ...)` troca a resposta do `429`.
- Acima do limite sai um `HttpException` `429` (`RateLimited`), no formato do renderer do app (`{"message": "Too Many Attempts."}` com o `LaravelErrorRenderer`).
- Os contadores ficam no `Repository` de cache que o container tiver (`instance(Repository::class, ...)`), senão no `Cache::repository()`. Com `RedisCacheStore`, o limite vale para todos os workers e servidores.
- Fora do HTTP, o mesmo `RateLimiter` tem `attempt()`, `hit()`, `tooManyAttempts()`, `remaining()`, `availableIn()` e `clear()`.

### Token bucket

`RateLimitMiddleware` aplica token bucket por chave e responde `429` em problem+json (`RateLimited`), com cabeçalhos de limite, restante e espera.

```php
use Nimbus\Middleware\RateLimitMiddleware;
use Nimbus\Redis\RedisRateLimitStore;
use Pam\Http\Request;

$app->middleware(new RateLimitMiddleware(
    requestsPerSecond: 20,
    burst: 40,
    store: new RedisRateLimitStore($redis),
    keyResolver: static fn (Request $request): string =>
        'token:' . $request->getHeader('authorization', 'anonymous'),
));
```

Cada consumo é um script Lua atômico. Atrás de proxy, passe um `keyResolver` que só confie no proxy que você configurou.

Em rotas de login e cadastro, um limite por rota:

```php
$throttle = new RateLimitMiddleware(requestsPerSecond: 1, burst: 10);
$router->post('login', [LoginController::class, 'store'])->middleware($throttle);
```

## Cache de resposta

`ResponseCacheMiddleware` guarda a resposta e a serve até o TTL vencer. A chave padrão é o hash de método, caminho e query string.

```php
use Nimbus\Middleware\ResponseCacheMiddleware;
use Nimbus\Redis\RedisResponseCacheStore;

$cache = new ResponseCacheMiddleware(new RedisResponseCacheStore($redis), ttlSeconds: 30);

$router->get('products', [ProductController::class, 'index'])->middleware($cache);
```

O terceiro argumento, `keyResolver`, troca a chave, por exemplo para separar por usuário. Não use cache compartilhado em respostas que dependem do usuário sem incluir o usuário na chave.

## Idempotência

`IdempotencyMiddleware` lê o cabeçalho `Idempotency-Key`. Uma repetição com a mesma chave recebe a resposta gravada em vez de executar o handler de novo, o que protege pagamentos e pedidos contra reenvio do cliente.

```php
use Nimbus\Middleware\IdempotencyMiddleware;
use Nimbus\Redis\RedisIdempotencyStore;

$router->post('payments', [PaymentController::class, 'store'])
    ->middleware(new IdempotencyMiddleware(new RedisIdempotencyStore($redis), ttlSeconds: 86_400));
```

Com `required: true` (padrão), a falta do cabeçalho responde `422`. A resposta repetida traz o cabeçalho `idempotency-replayed: true`. A mesma chave com outro método, caminho ou corpo responde `409` (`Conflict`), para que um cliente não reaproveite uma chave em outra operação.

## Stores próprios

Os três middlewares recebem o store por interface. Para guardar em outro lugar, implemente:

| Interface | Métodos |
| --- | --- |
| `Nimbus\RateLimit\RateLimitStore` | `consume(string $key, int $requestsPerSecond, int $capacity, float $now): RateLimitDecision` |
| `Nimbus\Cache\ResponseCacheStore` | `get(string $key, int $now): ?CacheRecord`, `put(string $key, CacheRecord $record)`, `forget(string $key)` |
| `Nimbus\Idempotency\IdempotencyStore` | `get(string $key, int $now): ?IdempotencyRecord`, `put(string $key, IdempotencyRecord $record)` |

`RateLimitDecision` traz `allowed`, `limit`, `remaining` e `retryAfterSeconds`, que viram os cabeçalhos da resposta. `consume()` precisa ser atômico entre workers; um `get` seguido de `put` deixa passar requisições a mais sob concorrência. `get()` dos outros dois devolve `null` para registros vencidos (`expiresAt` menor que `$now`).
