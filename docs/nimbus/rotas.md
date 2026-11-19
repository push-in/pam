# Rotas e middlewares

## O arquivo `routes.php`

Todas as rotas HTTP ficam em `routes.php`. O arquivo recebe o registrador de rotas como `$router` (uma instância de `Nimbus\RouteRegistrar`); não existe fachada `Route::` nem arrays de configuração.

```php
// index.php
$app = new Nimbus\App();
$app->routes(__DIR__ . '/routes.php');
$app->listen(3000);
```

```php
// routes.php
use App\Controllers\ProductController;
use App\Controllers\StatsController;
use App\Controllers\UserController;
use Nimbus\RouteRegistrar;
use Pam\Http\Request;
use Pam\Http\Response;

/** @var RouteRegistrar $router */

$router->get('ping', fn (Request $request, Response $response) => $response->json(['pong' => true]));

// rotas de um controller: cada rota nomeia só o método
$router->controller(ProductController::class, function (RouteRegistrar $router): void {
    $router->get('products', 'index');
    $router->get('products/{product}', 'show');
    $router->post('products', 'store')->auth();
    $router->post('products/{product}/publish', 'publish')->can('publish', 'product');
});

// CRUD: index, store, show, update e destroy; a closure é opcional e serve para rotas extras
$router->resource('users', UserController::class, function (RouteRegistrar $users): void {
    $users->post('{user}/ban', 'ban')->can('admin');
});

// rota avulsa: sempre [Controller::class, 'metodo']
$admin = $router->auth()->prefix('admin')->can('admin');
$admin->get('stats', [StatsController::class, 'index']);
```

O caminho pode vir com ou sem a barra inicial.

## Métodos

`get`, `post`, `put`, `patch`, `delete`, `head` e `options` existem no `RouteRegistrar` e no `App`. Todo `GET` também responde `HEAD` (o PAM descarta o corpo e mantém status e cabeçalhos). Sem rota `OPTIONS` explícita, o PAM responde `204` com o cabeçalho `Allow`. Um caminho que existe para outro método responde `405` com `Allow`; um caminho desconhecido responde `404`.

## Handlers

Um handler pode ser:

| Forma | Exemplo |
| --- | --- |
| Closure | `fn (Request $request, Response $response) => $response->json([...])` |
| Controller e método | `[ProductController::class, 'show']` |
| Método dentro de `controller()` | `'show'` |
| Classe invocável | `CreateProduct::class` (método `__invoke`) |

Closures que declaram só `($request, $response)` recebem os dois diretamente. Com qualquer outro parâmetro, a closure é resolvida pelo container, como as actions de controller:

```php
use Nimbus\Storage\Disk;

$router->get('avatars/{id}', static fn (string $id, Disk $disk, Response $response): Response =>
    $response->json(['url' => $disk->url("avatars/{$id}.png")]));
```

## `resource()` e `apiResource()`

`resource('users', UserController::class)` registra:

| Método | Caminho | Action |
| --- | --- | --- |
| GET | `/users` | `index` |
| POST | `/users` | `store` |
| GET | `/users/{user}` | `show` |
| PUT, PATCH | `/users/{user}` | `update` |
| DELETE | `/users/{user}` | `destroy` |

O nome do parâmetro é o singular do caminho. As rotas da closure ficam relativas ao caminho do recurso e ligadas ao mesmo controller. `apiResource()` registra as mesmas rotas sem closure.

## Parâmetros e restrições

Leia o parâmetro com `$request->route('product')` ou declare um parâmetro de mesmo nome na action (veja [Controllers](./controllers.md#parametros-de-rota)). `where()` restringe o formato:

```php
use Nimbus\RouteConstraint;

$router->get('products/{id}', [ProductController::class, 'show'])
    ->where('id', RouteConstraint::Integer)
    ->name('products.show');
```

`RouteConstraint` tem `Integer`, `Uuid`, `Ulid`, `Slug`, `Alpha` e `AlphaNumeric`; `where()` também aceita uma expressão regular própria (até 512 bytes).

O roteador tem limites fixados antes do app congelar: 10.000 rotas por padrão (configurável até 100.000), caminhos de até 2 KiB, 128 segmentos e 32 parâmetros por rota. Caminhos de requisição grandes demais nem chegam ao PCRE.

## Grupos

O registrador é imutável: `prefix()`, `middleware()`, `auth()`, `can()` e `signed()` devolvem um novo registrador, que pode ser guardado numa variável ou usado com `group()`:

```php
use Nimbus\Middleware\RateLimitMiddleware;

$router->prefix('api/v1')
    ->middleware(new RateLimitMiddleware(requestsPerSecond: 20, burst: 40))
    ->group(function (RouteRegistrar $router): void {
        $router->resource('orders', OrderController::class);
        $router->post('login', [LoginController::class, 'store']);
    });
```

| Método | Efeito nas rotas do grupo |
| --- | --- |
| `prefix('admin')` | prefixa o caminho |
| `middleware($m)` | adiciona middleware (objeto, closure, nome de classe ou lista) |
| `auth()` | exige um usuário autenticado ([Autenticação](./autenticacao.md)) |
| `can('ability', 'param')` | exige a permissão pelo Gate; implica `auth()` ([Autorização](./autorizacao.md)) |
| `signed()` | exige URL assinada ([Storage e URLs assinadas](./storage.md#urls-assinadas)) |

Os mesmos métodos existem numa rota só: `->auth()`, `->can()`, `->signed()`, `->middleware()`.

## Middlewares

Middlewares globais, de grupo e de rota usam o mesmo contrato do PAM (`Pam\Contracts\Http\MiddlewareInterface`) ou uma closure. Um nome de classe é resolvido pelo container.

```php
$app->middleware(new Nimbus\Middleware\SecurityHeadersMiddleware());   // global
$router->middleware(App\Middleware\Authenticate::class)->group(...);   // grupo
$router->get('me', [MeController::class, 'show'])->middleware(AuthenticateMiddleware::class); // rota
```

Um middleware próprio:

```php
namespace App\Middleware;

use Pam\Contracts\Http\MiddlewareInterface;
use Pam\Contracts\Http\RequestHandlerInterface;
use Pam\Http\Request;
use Pam\Http\Response;

final readonly class RequireJson implements MiddlewareInterface
{
    public function process(Request $request, Response $response, RequestHandlerInterface $next): Response
    {
        if ($request->method === 'POST' && !str_contains($request->getHeader('content-type', ''), 'json')) {
            return $response->json(['message' => 'Envie JSON.'], 415);
        }
        return $next->handle($request, $response);
    }
}
```

Middlewares incluídos em `Nimbus\Middleware`:

| Classe | Uso |
| --- | --- |
| `AuthenticateMiddleware` | autenticação pelo `Authenticator` do container |
| `AuthorizeMiddleware` | permissão pelo Gate (usado por `can()`) |
| `CorsMiddleware` | origens, métodos e cabeçalhos permitidos |
| `RateLimitMiddleware` | limite por chave com token bucket ([Redis](./redis.md#rate-limit)) |
| `IdempotencyMiddleware` | `Idempotency-Key` para POST repetido ([Redis](./redis.md#idempotencia)) |
| `ResponseCacheMiddleware` | cache de respostas ([Redis](./redis.md#cache-de-resposta)) |
| `SecurityHeadersMiddleware` | cabeçalhos de segurança conservadores |
| `DeadlineMiddleware` | prazo cooperativo por requisição |
| `TransactionalMiddleware` | envolve a requisição numa transação |
| `QueryBudgetMiddleware` | limite de queries por requisição ([Banco](./banco.md#orcamento-de-queries)) |
| `ResolveTenantMiddleware` | resolve o tenant da requisição |
| `ValidateSignatureMiddleware` | URL assinada (usado por `signed()`) |
| `StartSessionMiddleware`, `VerifyCsrfTokenMiddleware`, `EncryptCookiesMiddleware`, `MethodOverrideMiddleware` | apps web ([Sessão](./sessao.md)) |
