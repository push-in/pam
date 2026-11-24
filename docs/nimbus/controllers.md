# Controllers e container

## Controllers

Um controller é uma classe comum em `app/Controllers`. O container monta o controller (dependências do construtor) e injeta os parâmetros de cada action.

```php
namespace App\Controllers;

use App\Http\Requests\StoreProductRequest;
use App\Http\Resources\ProductResource;
use App\Models\Product;
use Nimbus\Database\ArcanoManager;
use Nimbus\Http\HttpException;
use Nimbus\Http\ProblemCode;
use Nimbus\Http\ResourceCollection;
use Nimbus\Pagination\Paginator;
use Pam\Http\Request;

final readonly class ProductController
{
    public function __construct(private ArcanoManager $arcano) {}

    public function index(Request $request): ResourceCollection
    {
        return ProductResource::collection(
            Paginator::paginate($this->arcano->select(Product::class)->orderBy('id'), $request),
        );
    }

    public function show(Request $request): ProductResource
    {
        $product = $this->arcano->select(Product::class)->wherePK((int) $request->route('id'))->fetchOne()
            ?? throw new HttpException(404, ProblemCode::NotFound, 'Product not found.');

        return new ProductResource($product);
    }

    public function store(StoreProductRequest $request): ProductResource
    {
        $product = new Product($request->name, $request->price);
        $this->arcano->entityManager()->persist($product)->run();

        return new ProductResource($product, 201);
    }
}
```

Uma action pode devolver:

- `Pam\Http\Response` (por exemplo `$response->json($dados, 201)`);
- um `Nimbus\Http\Responsable`: `JsonResource`, `ResourceCollection`, `View`, `Redirect`;
- nada, depois de escrever na `Response` recebida.

Parâmetros da action, em ordem de resolução: parâmetros de rota com o mesmo nome, `Request` e `Response` da requisição, `Input` e `FormRequest` já validados, e qualquer outra classe pelo container.

`pam make:controller ProductController` gera um controller com métodos nomeados, pronto para `[ProductController::class, 'index']`.

## Parâmetros de rota

Um parâmetro escalar com o mesmo nome do segmento recebe o valor da URL como string:

```php
$router->get('orders/{code}', [OrderController::class, 'show']);

public function show(string $code): Response { ... }
```

Um parâmetro tipado com uma classe é convertido em objeto por `bindRoute()` no container ou pela interface `Nimbus\Routing\RouteBindable`:

```php
use App\Models\Product;
use Nimbus\Container\Container;
use Nimbus\Database\ArcanoManager;

$app->container()->bindRoute(Product::class, static fn (string $id, Container $container): Product =>
    $container->get(ArcanoManager::class)->select(Product::class)->wherePK((int) $id)->fetchOne()
        ?? throw new HttpException(404, ProblemCode::NotFound, 'Product not found.'));
```

```php
$router->get('products/{product}', [ProductController::class, 'show']);

public function show(Product $product): ProductResource
{
    return new ProductResource($product);
}
```

O mesmo binding é usado por `can('update', 'product')` para carregar a entidade antes de chamar a policy.

## Contexto da requisição

O `$request->attributes->set()` do Laravel vira `Nimbus\Http\RequestContext`: um objeto por requisição, preenchido pelo middleware e lido pela action. Ele some quando a requisição termina.

```php
use Nimbus\Container\Container;
use Nimbus\Http\RequestContext;

final readonly class ResolveAccount implements MiddlewareInterface
{
    public function __construct(private Container $container, private Accounts $accounts)
    {
    }

    public function process(Request $request, Response $response, RequestHandlerInterface $next): Response
    {
        $context = $this->container->get(RequestContext::class);
        $context->put($this->accounts->fromToken($request));   // valor tipado
        $context->set('monitor', $this->accounts->monitor());  // valor por chave
        return $next->handle($request, $response);
    }
}

public function show(Account $account, RequestContext $context): array
{
    return ['account' => $account->id, 'monitor' => $context->get('monitor')];
}
```

Regra de resolução: um parâmetro tipado com uma classe recebe primeiro a instância que está no escopo da requisição (`RequestContext::put()` ou `scopedInstance()`), mesmo quando a rota tem um segmento com o mesmo nome (`{account}`). Sem essa instância, o segmento passa pelo route binding, e sem segmento o container monta o objeto.

## Container

```php
$container = $app->container();

$container->bind(UserRepository::class, DatabaseUserRepository::class);   // nova instância a cada resolução
$container->singleton(PriceTable::class);                                 // uma por worker
$container->scoped(CurrentUser::class);                                   // uma por requisição
$container->instance(Clock::class, new SystemClock());                    // objeto pronto
```

Workers do PAM vivem por muitas requisições. Um `singleton` sobrevive entre requisições, então não pode guardar estado do usuário. Valores `scoped` são criados uma vez por requisição e descartados no fim, mesmo quando o handler lança exceção. O plano de parâmetros de cada classe é compilado uma vez por worker, sem reflexão por requisição.

## Service providers

Um provider agrupa registros no container e na aplicação. Os providers do Nimbus (`ArcanoServiceProvider`, `StorageServiceProvider`, `SessionServiceProvider`, `ViewServiceProvider`, `TranslationServiceProvider`) seguem o mesmo contrato:

```php
namespace App\Providers;

use Nimbus\App;
use Pam\Contracts\Http\ApplicationInterface;
use Pam\Contracts\Package\ServiceProviderInterface;

final readonly class AppServiceProvider implements ServiceProviderInterface
{
    public function register(ApplicationInterface $application): void
    {
        if (!$application instanceof App) {
            throw new \InvalidArgumentException('AppServiceProvider requires Nimbus\\App.');
        }
        $application->container()->singleton(UserRepository::class, DatabaseUserRepository::class);
    }

    public function boot(ApplicationInterface $application): void
    {
    }
}
```

```php
$app->provider(new App\Providers\AppServiceProvider());
```

Com `new App()` (padrão `discoverPackages: true`), providers declarados por pacotes Composer são descobertos automaticamente. Os testes usam `new App(discoverPackages: false)` para montar só o necessário.

## Configuração tipada

Declare a configuração uma vez e falhe no boot quando faltar variável ou o valor estiver errado:

```php
use Nimbus\Config\ConfigDefinition;
use Nimbus\Config\ConfigType;
use Nimbus\Config\Configuration;

$config = Configuration::fromEnvironment([
    new ConfigDefinition('app.port', 'PAM_PORT', ConfigType::Integer),
    new ConfigDefinition('auth.secret', 'AUTH_SECRET', sensitive: true),
]);

$port = $config->integer('app.port');
```

Para diagnóstico, use `$config->redacted()`, que mascara os valores sensíveis. As mensagens de erro citam a variável e o tipo esperado, nunca o valor recebido. Tipos, valores padrão e as variáveis lidas por cada módulo estão em [Configuração tipada](./configuracao.md).

## Tratamento de erros

Sem configuração, exceções não capturadas viram [problem+json](./erros.md) e a mensagem vai para o log, nunca para o cliente. `HttpException` (inclusive `ValidationException`) sempre vira problem+json; para as demais exceções, `$app->onError()` troca o tratador padrão:

```php
$app->onError(static function (\Throwable $error, Response $response): Response {
    return $response->json(['message' => 'Erro interno.'], 500);
});
```
