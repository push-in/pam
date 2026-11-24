# Cache da aplicação e tags

`Nimbus\Cache\Repository` guarda valores calculados (consultas, respostas de APIs externas, contadores) num `Store`. Para guardar a resposta HTTP inteira de uma rota, use o `ResponseCacheMiddleware` ([Redis](./redis.md#cache-de-resposta)).

## Stores

| Store | Onde fica | Para |
| --- | --- | --- |
| `Nimbus\Cache\MemoryStore` | memória PHP do worker, até 10 000 chaves (as mais antigas saem primeiro) | desenvolvimento, testes, cache local de um worker |
| `Nimbus\Redis\RedisCacheStore` | Redis, com prefixo `nimbus:cache:` | produção com mais de um worker ou servidor |

```php
use Nimbus\Cache\Cache;
use Nimbus\Cache\Repository;
use Nimbus\Redis\RedisCacheStore;
use Nimbus\Redis\RedisPool;

$cache = new Repository(new RedisCacheStore(RedisPool::fromEnvironment()));

$app->container()->instance(Repository::class, $cache);
Cache::useRepository($cache);   // a fachada estática usa o mesmo repositório
```

O `MemoryStore` devolve o mesmo objeto que recebeu. O `RedisCacheStore` grava inteiros como inteiros do Redis (então `increment()` é atômico) e os outros valores com `serialize()`. Objetos só voltam como objetos para as classes listadas em `allowedClasses`; sem a lista, um objeto volta como `__PHP_Incomplete_Class`, e uma entrada gravada no Redis nunca instancia código:

```php
new RedisCacheStore($redis, prefix: 'shop:cache:', allowedClasses: [Money::class]);
```

Prefira guardar arrays e escalares.

## Ler e gravar

Injete `Repository` nos serviços:

```php
use Nimbus\Cache\Repository;

final readonly class ExchangeRates
{
    public function __construct(private Repository $cache, private RatesApi $api) {}

    public function brlPerUsd(): float
    {
        return $this->cache->remember('rates:usd-brl', 300, fn (): float => $this->api->fetch('USD', 'BRL'));
    }
}
```

| Método | Efeito |
| --- | --- |
| `get($key, $default)` | valor ou `$default` (uma closure como default só roda quando a chave falta) |
| `has($key)` | `true` quando a chave existe e não venceu |
| `put($key, $value, $ttl)` | grava; `$ttl` em segundos ou um `DateTimeInterface`; `null` grava sem prazo |
| `forever($key, $value)` | `put()` sem prazo |
| `add($key, $value, $ttl)` | grava só se a chave não existe; devolve `true` quando gravou |
| `remember($key, $ttl, $callback)` | valor guardado ou o resultado do callback, gravado pelo TTL |
| `rememberForever($key, $callback)` | `remember()` sem prazo |
| `pull($key, $default)` | lê e apaga |
| `increment($key, $by)`, `decrement($key, $by)` | contador; devolve o novo valor |
| `forget($key)` | apaga a chave |
| `flush()` | apaga todas as chaves do store (no Redis, só as do prefixo) |

`null` significa ausência: `remember()` não grava um resultado `null`, e um valor `null` gravado é lido como chave inexistente. Um TTL `0` ou negativo apaga a chave. Chaves vazias lançam `InvalidArgumentException`.

`add()` serve como trava simples entre workers, porque no Redis é um `SET NX`:

```php
if ($cache->add('lock:report:' . $date, 1, 60)) {
    $reports->build($date);
}
```

## Tags

`tags()` devolve uma visão do cache cujas chaves pertencem às tags. `flush()` nessa visão invalida só as chaves gravadas com aquelas tags:

```php
$cache->tags(['products'])->put('page:1', $page, 600);
$cache->tags(['products', 'user:' . $userId])->put('recommended', $items, 600);

$cache->tags('products')->get('page:1');

// Depois de alterar um produto:
$cache->tags('products')->flush();
```

Depois do `flush()` de `products`, as duas chaves acima deixam de ser lidas; chaves sem tag e chaves só com outras tags continuam.

A chave com tags é lida com o mesmo conjunto de tags com que foi gravada: `$cache->tags('products')->get('recommended')` não encontra o valor gravado com `['products', 'user:7']`. Uma chave com tag também não aparece em `$cache->get()`. `tags()` numa visão com tags acrescenta tags ao conjunto.

### Como a invalidação funciona

Cada tag tem uma versão aleatória guardada no próprio store (`tag:<nome>:version`), e a chave real é prefixada pelo hash das versões das suas tags. `flush()` troca a versão: as chaves antigas nunca mais são lidas e somem pelo TTL ou pela política de memória do Redis. Por isso `flush()` com tags não percorre chaves e custa uma escrita por tag, em qualquer store. Grave valores com tag sempre com TTL, para que as versões antigas não ocupem memória sem prazo.

## Fachada `Cache`

`Nimbus\Cache\Cache` expõe os mesmos métodos de forma estática (`get`, `has`, `put`, `forever`, `add`, `remember`, `pull`, `increment`, `forget`, `flush`, `tags`, `lock`, `restoreLock`) sobre o repositório de `Cache::useRepository()`. Sem repositório configurado, usa um `MemoryStore` do worker.

```php
use Nimbus\Cache\Cache;

Cache::tags(['users', 'user:' . $id])->put('profile:' . $id, $profile, 600);
Cache::tags(['user:' . $id])->flush();
```

Em serviços, prefira injetar `Repository`: a dependência fica visível e o teste troca o store pelo construtor.

## Locks

`lock()` dá um lock atômico com dono, como o `Cache::lock()` do Laravel. No `RedisCacheStore` é `SET NX EX` com liberação por Lua (só o dono libera), compartilhado entre workers e servidores; nos outros stores usa o `add()` do store (o `MemoryStore` vale para um worker).

```php
use Nimbus\Cache\Cache;
use Nimbus\Cache\LockTimeoutException;

// roda sob o lock e libera no fim; false se outro dono tem o lock
Cache::lock('ask:' . $accountId, 30)->get(fn () => $this->ask($accountId));

// espera até 5 s pelo lock
try {
    Cache::lock('heartbeat', 10)->block(5, fn () => $this->beat());
} catch (LockTimeoutException) {
    // outro processo segurou o lock por mais de 5 s
}

// liberar em outro processo (um job)
$lock = $cache->lock('report', 120);
$lock->get();
$queue->dispatch(new BuildReport($lock->owner()));
// no job: $cache->restoreLock('report', $owner)->release();
```

`$seconds = 0` mantém o lock até `release()`. `block()` espera com `Pam\Async\delay()` dentro do PAM, sem travar as outras requisições do worker. O agendador usa o mesmo mecanismo com `CacheSchedulerLock($repository)`; o `RedisSchedulerLock` usa `RedisLock`.

## Testes

`Nimbus\Testing\TestCase` restaura a fachada no fim de cada teste (`Cache::useRepository(null)`). Num teste, use um repositório em memória:

```php
$cache = new Repository(new MemoryStore());
$this->application()->container()->instance(Repository::class, $cache);
```
