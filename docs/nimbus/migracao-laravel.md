# Migração do Laravel

O Nimbus segue o mapa de pastas do Laravel e muitos nomes de método, mas não é compatível no código: não há fachadas no código de produção, o ORM é outro (data mapper, não Active Record) e as rotas ficam num arquivo só. Para rodar um app Laravel sem reescrever, use o PAM como servidor do próprio Laravel ([PAM com Laravel](https://github.com/push-in/pam/blob/main/docs/laravel.md)); esta página é para quem vai portar o código para o Nimbus.

Cada seção mostra o código Laravel e o equivalente Nimbus lado a lado. O que não tem equivalente está em [Sem equivalente por enquanto](#sem-equivalente-por-enquanto).

## Diferenças de modelo

- **Workers persistentes**: o app fica carregado entre requisições. Nada de estado do usuário em `singleton` ou propriedade estática; use `scoped` ou passe pelo `Request`. Veja [Workers persistentes](#workers-persistentes).
- **Sem fachadas**: dependências chegam pelo construtor ou pelos parâmetros da action. As poucas classes estáticas (`Http`, `Cache`, `Hash`) são por worker e não guardam estado da requisição.
- **Data mapper**: entidades são classes simples; quem lê e grava é o `ArcanoManager`, não a própria entidade.
- **Rotas num arquivo**: `routes.php` com `$router`, sem `Route::`. Grupos são closures ou registradores guardados em variável.
- **Erros em problem+json** com `code` inteiro estável ([Erros](./erros.md)).
- **Tudo é registrado no boot**: provider, container e rotas são montados uma vez por worker em `bootstrap/app.php`; depois o app congela.

## Conversão automática com Rector

O pacote `pushinbr/nimbus-rector` (código em [`packages/nimbus-rector`](https://github.com/push-in/pam/tree/main/packages/nimbus-rector)) traz um conjunto de regras do [Rector](https://getrector.com) para a parte mecânica da migração. Enquanto não estiver no Packagist, instale pelo caminho do repositório:

```bash
pam composer config repositories.nimbus-rector path /caminho/do/pam/packages/nimbus-rector
pam composer require --dev pushinbr/nimbus-rector:@dev
```

```php
// rector.php
use Nimbus\Rector\Set\NimbusSetList;

return Rector\Config\RectorConfig::configure()
    ->withPaths([__DIR__ . '/app', __DIR__ . '/routes'])
    ->withSets([NimbusSetList::LARAVEL_TO_NIMBUS])
    ->withImportNames(importShortClasses: false, removeUnusedImports: true);
```

```bash
vendor/bin/rector process --dry-run   # mostra o diff
vendor/bin/rector process             # aplica
```

O que o conjunto reescreve:

| Regra | Antes (Laravel) | Depois (Nimbus) |
| --- | --- | --- |
| `RenameClassRector` | `Illuminate\Http\Request`, `JsonResponse`, `Response`, `FormRequest`, `JsonResource`, `ResourceCollection`, fachadas `Http`, `Cache`, `Hash` e as classes do cliente HTTP | `Pam\Http\Request`, `Pam\Http\Response`, `Nimbus\Validation\FormRequest`, `Nimbus\Http\JsonResource`, `Nimbus\Http\ResourceCollection`, `Nimbus\Http\Client\Http`, `Nimbus\Cache\Cache`, `Nimbus\Security\Hash` |
| `LaravelRequestCallToNimbusRector` | `$request->query('x')`, `header('x')`, `header()`, `getContent()`, `method()` | `getQuery('x')`, `getHeader('x')`, `headers()`, `body()`, `->method` |
| | `$request->input()`, `all()`, `has()`, `filled()`, `only()`, `bearerToken()`, `json('x')` | `Nimbus\Http\RequestData::input($request, ...)` e equivalentes |
| `ResponseHelperToResponseParameterRector` | `response()->json($d, 201)`, `response()->noContent()` | `$response->json($d, 201)`, `$response->status(204)`, e acrescenta `Response $response` à action |
| `RouteFacadeToRouterRector` | `Route::get(...)`, `Route::prefix('v1')->group(function () {...})`, `->middleware('auth:sanctum')` | `$router->get(...)`, `->group(function (RouteRegistrar $router) {...})`, `->auth()` |
| `AbortToHttpExceptionRector` | `abort(404, 'x')`, `abort_if()`, `abort_unless()` | `throw new HttpException(404, ProblemCode::NotFound, 'x')` |

Ficam para revisão manual: `response()->json()` com cabeçalhos ou opções, `response()` dentro de middleware do Laravel, `Route::group([...])` com array, `Route::controller()`, middlewares por nome (`throttle:...`), `abort()` com status sem `ProblemCode` (402, por exemplo), entidades, queries, jobs, validação e autenticação. O `routes/api.php` do Laravel ganha o prefixo `api` automaticamente e o Nimbus não: envolva o arquivo convertido em `$router->prefix('api')->group(...)`.

No `buck-log-intelligence` (29 rotas, 11 controllers), o `--dry-run` altera 17 arquivos de `app/Http` e o `routes/api.php`.

## Rotas

`routes/api.php` e `routes/web.php` viram um `routes.php` que recebe `$router` (`Nimbus\RouteRegistrar`). O registrador é imutável: `prefix()`, `middleware()`, `auth()` e `can()` devolvem um novo registrador. Detalhes em [Rotas e middlewares](./rotas.md).

::: code-group

```php [Laravel]
// routes/api.php
use App\Http\Controllers\OrderController;
use App\Http\Controllers\ProductController;
use Illuminate\Support\Facades\Route;

Route::get('/ping', fn () => response()->json(['pong' => true]));

Route::middleware('auth:sanctum')->prefix('v1')->group(function () {
    Route::apiResource('orders', OrderController::class);
    Route::post('orders/{order}/cancel', [OrderController::class, 'cancel'])
        ->middleware('can:cancel,order');
});

Route::controller(ProductController::class)->group(function () {
    Route::get('products', 'index');
    Route::get('products/{id}', 'show')->whereNumber('id')->name('products.show');
});
```

```php [Nimbus]
// routes.php
use App\Controllers\OrderController;
use App\Controllers\ProductController;
use Nimbus\RouteConstraint;
use Nimbus\RouteRegistrar;
use Pam\Http\Request;
use Pam\Http\Response;

/** @var RouteRegistrar $router */

$router->get('ping', fn (Request $request, Response $response) => $response->json(['pong' => true]));

$router->auth()->prefix('v1')->group(function (RouteRegistrar $router): void {
    $router->apiResource('orders', OrderController::class);
    $router->post('orders/{order}/cancel', [OrderController::class, 'cancel'])
        ->can('cancel', 'order');
});

$router->controller(ProductController::class, function (RouteRegistrar $router): void {
    $router->get('products', 'index');
    $router->get('products/{id}', 'show')->where('id', RouteConstraint::Integer)->name('products.show');
});
```

:::

- `resource()` registra `index`, `store`, `show`, `update` e `destroy` (sem `create` e `edit`) e aceita uma closure para rotas extras do recurso; `apiResource()` registra as mesmas cinco sem closure.
- `whereNumber`, `whereUuid`, `whereUlid`, `whereAlpha`, `whereAlphaNumeric` viram `where('x', RouteConstraint::Integer|Uuid|Ulid|Alpha|AlphaNumeric)`; `RouteConstraint::Slug` também existe e `where()` aceita uma regex.
- `->name()` existe por rota (vira o `operationId` do OpenAPI), mas não há `->name('v1.')` em grupo nem helper `route('nome')` para gerar URL.
- `->middleware('signed')` vira `->signed()`; `ThrottleRequests` vira `RateLimitMiddleware` ([Redis](./redis.md#rate-limit)).

### Route model binding

O binding implícito do Laravel (`Order $order` carregado pelo ID) precisa ser declarado uma vez por classe:

::: code-group

```php [Laravel]
// implícito: basta tipar o parâmetro
public function show(Order $order): OrderResource
{
    return new OrderResource($order);
}

// ou explícito
Route::bind('order', fn (string $value) => Order::where('code', $value)->firstOrFail());
```

```php [Nimbus]
// bootstrap/app.php
use App\Models\Order;
use Nimbus\Container\Container;
use Nimbus\Database\ArcanoManager;
use Nimbus\Http\HttpException;
use Nimbus\Http\ProblemCode;

$app->container()->bindRoute(Order::class, static fn (string $id, Container $container): Order =>
    $container->get(ArcanoManager::class)->select(Order::class)->wherePK((int) $id)->fetchOne()
        ?? throw new HttpException(404, ProblemCode::NotFound, 'Order not found.'));

// controller: igual ao Laravel
public function show(Order $order): OrderResource
{
    return new OrderResource($order);
}
```

:::

A entidade também pode implementar `Nimbus\Routing\RouteBindable`. O mesmo binding carrega a entidade para `can('cancel', 'order')`.

## Controllers, Request e Response

O controller é montado pelo container; a action recebe parâmetros de rota pelo nome, `Request`, `Response`, `Input`/`FormRequest` já validados, o `Principal` autenticado e qualquer classe do container. `Pam\Http\Request` é menor que o do Laravel: não há `input()`, `all()`, `user()` nem `bearerToken()`; `Nimbus\Http\RequestData` cobre as leituras de input para código portado. Veja [Controllers e container](./controllers.md).

::: code-group

```php [Laravel]
namespace App\Http\Controllers;

use App\Services\OrderSearch;
use Illuminate\Http\JsonResponse;
use Illuminate\Http\Request;

final class OrderController extends Controller
{
    public function __construct(private OrderSearch $search) {}

    public function index(Request $request): JsonResponse
    {
        $status = $request->query('status', 'open');
        $limit = (int) $request->input('limit', 20);
        $tenant = $request->header('X-Tenant');
        $token = $request->bearerToken();
        $user = $request->user();

        $orders = $this->search->find($user->id, $status, $limit);

        return response()->json(['data' => $orders], 200)
            ->header('X-Total', (string) count($orders));
    }

    public function store(Request $request): JsonResponse
    {
        $payload = $request->json()->all();
        $sku = $request->input('items.0.sku');
        // ...
        return response()->json($order, 201);
    }
}
```

```php [Nimbus]
namespace App\Controllers;

use App\Services\OrderSearch;
use Nimbus\Auth\Principal;
use Pam\Http\Request;
use Pam\Http\Response;

final readonly class OrderController
{
    public function __construct(private OrderSearch $search) {}

    // rota com ->auth(), para o Principal existir
    public function index(Request $request, Response $response, Principal $user): Response
    {
        $status = $request->getQuery('status', 'open');
        $limit = (int) $request->getQuery('limit', 20);
        $tenant = $request->getHeader('x-tenant');
        $authorization = $request->getHeader('authorization');   // "Bearer ..." inteiro

        $orders = $this->search->find($user->identifier(), $status, $limit);

        return $response->header('X-Total', (string) count($orders))
            ->json(['data' => $orders]);
    }

    public function store(Request $request, Response $response): Response
    {
        $payload = $request->json();                 // corpo JSON inteiro, decodificado
        $sku = $payload['items'][0]['sku'] ?? null;
        // ...
        return $response->json($order, 201);
    }
}
```

:::

| Laravel | Nimbus |
| --- | --- |
| `$request->query('x', $d)` | `$request->getQuery('x', $d)`; `$request->query()` devolve todas |
| `$request->input('x', $d)` | `RequestData::input($request, 'x', $d)` (corpo sobre query, notação com ponto), ou um [`Input`](#validacao) |
| `$request->input('x')` (formulário) | `Nimbus\Http\FormData::field($request, 'x')`; `FormData::parse($request)` devolve todos |
| `$request->all()`, `has()`, `filled()`, `only()` | `RequestData::all($request)`, `has()`, `filled()`, `only()` |
| `$request->header('X-Y')` | `$request->getHeader('x-y', $default)`; `$request->headers()` |
| `$request->bearerToken()` | `RequestData::bearerToken($request)` ou um `Authenticator` |
| `$request->user()` | parâmetro `Principal $user` (só o identificador e as abilities; carregue a entidade pelo repositório) |
| `$request->route('id')` | `$request->route('id')` ou parâmetro `string $id` |
| `$request->method()`, `path()` | propriedades `$request->method` e `$request->path` |
| `$request->getContent()` | `$request->body()` |
| `$request->cookie('x')` | `Nimbus\Http\RequestCookies::fromRequest($request)->get('x')` |
| `$request->ip()` | `(new Nimbus\Http\ClientIpResolver($proxiesConfiaveis))->resolve($request)` |
| `response()->json($d, 201)` | `$response->json($d, 201)` |
| `response($texto, 200)` | `$response->status(200)->send($texto)` |
| `->header('X', 'y')` | `$response->header('X', 'y')`; `addHeader()` para repetir |
| `response()->stream(...)` | `$response->stream($chunks, $contentType)` ou `streamOutput()` |
| `redirect('/x')`, `back()` | `Redirect::to('/x')`, `Redirect::back($request)` |
| `abort(404)` | `throw new HttpException(404, ProblemCode::NotFound, '...')` |
| `JsonResource`, `ResourceCollection` | `Nimbus\Http\JsonResource` / `ResourceCollection` com `toArray(Request $request)` ([Respostas](./respostas.md)) |

## Validação

O `FormRequest` do Laravel tem dois destinos: `Nimbus\Validation\Input`, com propriedades tipadas e atributos (preferido), ou `Nimbus\Validation\FormRequest`, com regras em array. Veja [Input e validação](./validacao.md).

::: code-group

```php [Laravel]
namespace App\Http\Requests;

use App\Enums\OrderChannel;
use Illuminate\Foundation\Http\FormRequest;
use Illuminate\Validation\Rule;

final class StoreOrderRequest extends FormRequest
{
    public function authorize(): bool
    {
        return $this->user()->can('orders:write');
    }

    public function rules(): array
    {
        return [
            'customer_email' => 'required|email',
            'note' => 'nullable|string|max:500',
            'quantity' => 'required|integer|min:1|max:99',
            'channel' => ['required', Rule::enum(OrderChannel::class)],
            'coupon' => 'nullable|regex:/^[A-Z0-9]{6}$/',
        ];
    }

    public function messages(): array
    {
        return ['quantity.min' => 'Peça ao menos uma unidade.'];
    }
}

// controller
public function store(StoreOrderRequest $request)
{
    $data = $request->validated();
    $email = $request->input('customer_email');
}
```

```php [Nimbus]
namespace App\Http\Requests;

use App\Enums\OrderChannel;
use Nimbus\Validation\Attributes\Email;
use Nimbus\Validation\Attributes\Field;
use Nimbus\Validation\Attributes\Max;
use Nimbus\Validation\Attributes\Min;
use Nimbus\Validation\Attributes\Regex;
use Nimbus\Validation\Input;
use Pam\Http\Request;

final class StoreOrderRequest extends Input
{
    #[Field('customer_email'), Email]
    public string $customerEmail;

    #[Max(500)]
    public ?string $note = null;            // nullable: opcional

    #[Min(1), Max(99)]
    public int $quantity;                   // sem valor padrão: obrigatório

    public OrderChannel $channel;           // enum: o valor precisa ser um dos cases

    #[Regex('/^[A-Z0-9]{6}$/')]
    public ?string $coupon = null;

    public function authorize(Request $request): bool
    {
        return $request->getHeader('x-team') === 'sales';
    }
}

// controller: o objeto chega validado
public function store(StoreOrderRequest $request): Response
{
    $email = $request->customerEmail;
    $data = $request->toArray();
}
```

:::

- Regras em atributos: `Min`, `Max`, `Email`, `Url`, `Uuid`, `Regex`, `In` e `Field` (nome da chave no payload). Tipos PHP fazem o papel de `string`, `integer`, `boolean`, `array`, `date` (`\DateTimeImmutable`) e `enum`. `Input` aninhado substitui as regras com ponto (`address.city`).
- Não há `messages()`: as mensagens são as padrão em inglês. Para um texto próprio, escreva uma regra (`Nimbus\Validation\ValidationRule`, que devolve a mensagem) ou lance `new ValidationException(['campo' => ['mensagem']])`.
- `authorize()` do `Input` recebe o `Request`, que não tem `user()`; para checar permissão do usuário, use `->can()` na rota ou o `Gate` na action.
- `unique:users,email`, `exists:` e as regras que consultam o banco não existem: confira no serviço e lance `ValidationException`.
- `Nimbus\Validation\FormRequest` aceita só arrays de regras (sem a sintaxe `'a|b'`) e só `required`, `string`, `integer`, `boolean`, `array`, `email`, `Rule::enum()` e objetos `ValidationRule`; seu `authorize()` não recebe parâmetros. Serve para portar rápido; depois troque por `Input`.
- O erro é `422` em problem+json com `errors` por campo, no mesmo formato de mapa do Laravel.

## Eloquent para Arcano

O Arcano é um data mapper: a entidade não tem `save()`, `find()` nem relações mágicas. Quem consulta e grava é o `Nimbus\Database\ArcanoManager`. Veja [Banco de dados com Arcano](./banco.md).

### Entidades

::: code-group

```php [Laravel]
namespace App\Models;

use Illuminate\Database\Eloquent\Model;
use Illuminate\Database\Eloquent\Relations\BelongsTo;
use Illuminate\Database\Eloquent\Relations\HasMany;

class Order extends Model
{
    protected $fillable = ['code', 'total_in_cents', 'paid_at'];
    protected $casts = ['paid_at' => 'immutable_datetime'];

    public function customer(): BelongsTo
    {
        return $this->belongsTo(Customer::class);
    }

    public function items(): HasMany
    {
        return $this->hasMany(OrderItem::class);
    }
}
```

```php [Nimbus]
namespace App\Models;

use Arcano\Annotated\Annotation\Column;
use Arcano\Annotated\Annotation\Entity;
use Arcano\Annotated\Annotation\Relation\BelongsTo;
use Arcano\Annotated\Annotation\Relation\HasMany;

#[Entity(role: 'order', table: 'orders')]
class Order
{
    #[Column(type: 'primary')]
    public ?int $id = null;

    #[Column(type: 'string(20)')]
    public string $code = '';

    #[Column(type: 'integer', name: 'total_in_cents')]
    public int $totalInCents = 0;

    #[Column(type: 'datetime', name: 'paid_at', nullable: true)]
    public ?\DateTimeImmutable $paidAt = null;

    #[BelongsTo(target: Customer::class)]
    public Customer $customer;

    /** @var list<OrderItem> */
    #[HasMany(target: OrderItem::class, collection: 'array')]
    public array $items = [];
}
```

:::

`$fillable` e `$casts` somem: as colunas são as propriedades com `#[Column]` e o tipo vem do atributo. Accessors e mutators viram métodos comuns da classe.

### Consultas e escrita

::: code-group

```php [Laravel]
$order = Order::find($id);
$order = Order::with('items')->findOrFail($id);

$recent = Order::where('total_in_cents', '>', 10_000)
    ->orderByDesc('id')
    ->limit(20)
    ->get();

$order = Order::where('code', 'A-1')->first();
$count = Order::whereNull('paid_at')->count();

$order->paid_at = now();
$order->save();
$order->delete();

DB::table('audit_log')->insert(['event' => 'paid']);
```

```php [Nimbus]
use Nimbus\Database\ArcanoManager;

/** @var ArcanoManager $arcano  (injetado no construtor) */
$order = $arcano->select(Order::class)->wherePK($id)->fetchOne();
$order = $arcano->select(Order::class)->load('items')->wherePK($id)->fetchOne()
    ?? throw new HttpException(404, ProblemCode::NotFound, 'Order not found.');

$recent = $arcano->select(Order::class)
    ->where('totalInCents', '>', 10_000)
    ->orderBy('id', 'DESC')
    ->limit(20)
    ->fetchAll();

$order = $arcano->repository(Order::class)->findOne(['code' => 'A-1']);
$count = $arcano->select(Order::class)->where('paidAt', null)->count();

$order->paidAt = new \DateTimeImmutable();
$arcano->entityManager()->persist($order)->run();
$arcano->entityManager()->delete($order)->run();

$arcano->database()->table('audit_log')->insertOne(['event' => 'paid']);
```

:::

- `where()` usa o nome da propriedade, não da coluna.
- `->with('items')` do Eloquent vira `->load('items')` (no Arcano, `with()` é join sem carregar).
- `repository(Order::class)` tem `findByPK()`, `findOne($escopo)` e `findAll($escopo, $ordem)`.
- Não há `firstOrFail()`: use `?? throw new HttpException(404, ...)`.

### Repositórios

Em vez de scopes do Eloquent (`scopePaid()`), escreva um repositório comum (`pam make:repository OrderRepository`) que recebe o `ArcanoManager`:

```php
namespace App\Repositories;

use App\Models\Order;
use Nimbus\Database\ArcanoManager;

final readonly class OrderRepository
{
    public function __construct(private ArcanoManager $arcano) {}

    /** @return list<Order> */
    public function unpaidFor(int $customerId): array
    {
        return $this->arcano->select(Order::class)
            ->where('customer.id', $customerId)
            ->where('paidAt', null)
            ->fetchAll();
    }
}
```

Os scopes globais do Eloquent (`addGlobalScope`) correspondem a `Arcano\ORM\Select\ScopeInterface`; o caso de multi-tenant já vem pronto em `$arcano->protectTenant()` ([Banco](./banco.md#multi-tenant)).

### Transações e paginação

::: code-group

```php [Laravel]
DB::transaction(function () use ($order) {
    $order->save();
    Payment::create([...]);
});

return OrderResource::collection(
    Order::where('customer_id', $id)->orderBy('id')->paginate(20)
);

Order::orderBy('id')->cursorPaginate(20);
```

```php [Nimbus]
use Nimbus\Pagination\Paginator;

$arcano->transaction(function () use ($arcano, $order, $payment): void {
    $arcano->entityManager()->persist($order)->persist($payment)->run();
});

return OrderResource::collection(
    Paginator::paginate($arcano->select(Order::class)->where('customer.id', $id)->orderBy('id'), $request, 20)
);

Paginator::cursor($arcano->select(Order::class)->orderBy('id'), $request, 20);
```

:::

`Paginator::paginate()` lê `?page=` e devolve `meta` com `current_page`, `per_page`, `last_page` e `total`, e `links` com `first`, `prev`, `next` e `last`. Para envolver a requisição inteira numa transação, use `TransactionalMiddleware` na rota ou no grupo.

### Migrations

O Arcano gera migrations pela diferença entre as entidades e o banco, então o fluxo comum é alterar a entidade e gerar:

::: code-group

```php [Laravel]
// php artisan make:migration create_orders_table
return new class extends Migration
{
    public function up(): void
    {
        Schema::create('orders', function (Blueprint $table) {
            $table->id();
            $table->string('code', 20)->unique();
            $table->integer('total_in_cents');
            $table->timestamp('paid_at')->nullable();
        });
    }

    public function down(): void
    {
        Schema::dropIfExists('orders');
    }
};
```

```php [Nimbus]
// gerada por ArcanoMigrationManager::generate(), ou escrita à mão
namespace Migration;

use Arcano\Migrations\Migration;

class CreateOrders extends Migration
{
    protected const DATABASE = 'default';

    public function up(): void
    {
        $this->table('orders')
            ->addColumn('id', 'primary', ['nullable' => false])
            ->addColumn('code', 'string', ['nullable' => false, 'size' => 20])
            ->addColumn('total_in_cents', 'integer', ['nullable' => false])
            ->addColumn('paid_at', 'datetime', ['nullable' => true])
            ->addIndex(['code'], ['unique' => true])
            ->setPrimaryKeys(['id'])
            ->create();
    }

    public function down(): void
    {
        $this->table('orders')->drop();
    }
}
```

:::

| Laravel | Nimbus |
| --- | --- |
| `php artisan make:migration` | `pam make:migration`, ou `$migrations->generate()` pela diferença |
| `php artisan migrate` | `pam exec bin/migrate` (template web) ou `$migrations->migrate()` |
| `migrate:rollback` | `pam exec bin/migrate rollback 1` ou `$migrations->rollback(1)` |
| `migrate:status` | `pam exec bin/migrate status` ou `$migrations->status()` |
| `php artisan db:seed` | `pam exec bin/migrate seed` ou `SeederRunner` |

`$migrations` é o `Nimbus\Database\ArcanoMigrationManager` do container.

### Factories e seeders

::: code-group

```php [Laravel]
class OrderFactory extends Factory
{
    public function definition(): array
    {
        return [
            'code' => fake()->bothify('A-####'),
            'total_in_cents' => fake()->numberBetween(100, 100_000),
            'customer_id' => Customer::factory(),
        ];
    }

    public function paid(): static
    {
        return $this->state(['paid_at' => now()]);
    }
}

Order::factory()->count(3)->paid()->create();
```

```php [Nimbus]
namespace Database\Factories;

use App\Models\Order;
use Nimbus\Database\Factory;

/** @extends Factory<Order> */
final class OrderFactory extends Factory
{
    protected string $model = Order::class;

    public function definition(): array
    {
        return [
            'code' => $this->faker->bothify('A-####'),
            'totalInCents' => $this->faker->numberBetween(100, 100_000),
            'customer' => CustomerFactory::new(),
        ];
    }

    public function paid(): self
    {
        return $this->state(['paidAt' => new \DateTimeImmutable()]);
    }
}

OrderFactory::new()->count(3)->paid()->create();
```

:::

As chaves são nomes de propriedade, e relações recebem outra factory. Seeders estendem `Nimbus\Database\Seeder`, com `run()` resolvido pelo container e `call([...])`. Veja [Factories e seeders](./factories.md).

## Middleware

::: code-group

```php [Laravel]
namespace App\Http\Middleware;

use Closure;
use Illuminate\Http\Request;

final class RequireTeam
{
    public function handle(Request $request, Closure $next, string $team)
    {
        if ($request->header('X-Team') !== $team) {
            return response()->json(['message' => 'Forbidden.'], 403);
        }
        return $next($request);
    }

    public function terminate(Request $request, $response): void
    {
        logger()->info('done', ['path' => $request->path()]);
    }
}

// bootstrap/app.php
->withMiddleware(function (Middleware $middleware) {
    $middleware->appendToGroup('api', [RequireJson::class]);
    $middleware->alias(['team' => RequireTeam::class]);
})

Route::middleware('team:catalog')->group(...);
```

```php [Nimbus]
namespace App\Middleware;

use Pam\Contracts\Http\MiddlewareInterface;
use Pam\Contracts\Http\RequestHandlerInterface;
use Pam\Http\Request;
use Pam\Http\Response;

final readonly class RequireTeam implements MiddlewareInterface
{
    public function __construct(private string $team) {}

    public function process(Request $request, Response $response, RequestHandlerInterface $next): Response
    {
        if ($request->getHeader('x-team') !== $this->team) {
            return $response->json(['message' => 'Forbidden.'], 403);
        }
        return $next->handle($request, $response);
    }
}

// routes.php: o "grupo" é um registrador guardado numa variável
$api = $router->middleware([RequireJson::class, new RequireTeam('catalog')]);
$api->get('products', [ProductController::class, 'index']);
```

:::

- O middleware recebe e devolve `Response`, e chama `$next->handle($request, $response)`.
- Parâmetros (`team:catalog`) viram argumentos do construtor; não há aliases por string. Um nome de classe sem parâmetros é resolvido pelo container.
- Globais: `$app->middleware(...)`. Grupos `web`/`api`: registradores em variável ou `group()`.
- **Terminable**: não existe `terminate()`. O mais próximo é um `Nimbus\Lifecycle\RequestLifecycleObserver` (`starting()` e `finished($request, $response, $falha)`), registrado com `$app->observe()`. Ele roda no fim do processamento, dentro da requisição; trabalho demorado vai para a [fila](#filas).

## Autenticação e autorização

Um `Nimbus\Auth\Authenticator` registrado no container transforma a requisição em `Principal` (`identifier()` e `can($ability)`). `->auth()` na rota exige o principal, que a action recebe como parâmetro. Veja [Autenticação](./autenticacao.md) e [Autorização](./autorizacao.md).

### Tokens pessoais (Sanctum)

::: code-group

```php [Laravel]
// emitir
$token = $user->createToken('cli', ['orders:read'])->plainTextToken;

// proteger
Route::middleware('auth:sanctum')->get('/orders', [OrderController::class, 'index']);

// checar
if ($request->user()->tokenCan('orders:read')) { /* ... */ }

// revogar
$user->tokens()->delete();
```

```php [Nimbus]
use Nimbus\Auth\Authenticator;
use Nimbus\Auth\PersonalAccessTokenAuthenticator;
use Nimbus\Auth\PersonalAccessTokens;
use Nimbus\Database\ArcanoDatabaseProvider;

// bootstrap/app.php
$tokens = new PersonalAccessTokens($app->container()->get(ArcanoDatabaseProvider::class));
$app->container()->instance(PersonalAccessTokens::class, $tokens);
$app->container()->instance(Authenticator::class, new PersonalAccessTokenAuthenticator($tokens));

// emitir (o texto do token aparece uma vez, em $new->plainTextToken)
$new = $tokens->create('user:' . $user->id, 'cli', ['orders:read']);
return $response->json($new->toArray(), 201);

// proteger
$router->get('orders', [OrderController::class, 'index'])->can('orders:read');

// checar
if ($principal->can('orders:read')) { /* ... */ }

// revogar
$tokens->revokeAll('user:' . $user->id);
```

:::

As tabelas vêm da migration `Nimbus\Auth\AuthTablesMigration` (`$migrations->publish('create_auth_tables', ...)`). O formato do token é `nbp_{id}.{segredo}`, e o banco guarda só o SHA-256.

### Bearer assinado, JWT e guards

| Laravel | Nimbus |
| --- | --- |
| Passport, `tymon/jwt-auth` | `JwtCodec` + `JwtAuthenticator` (`firebase/php-jwt`), ou `HmacTokenCodec` + `BearerTokenAuthenticator` sem banco |
| access + refresh token | `TokenPairIssuer` com `RefreshTokens`, com detecção de reuso |
| vários guards (`auth:api,sanctum`) | `ChainAuthenticator` com os autenticadores em ordem; não há guards nomeados |
| `Auth::login($user)` (web) | `$sessions->login($principal)` no `SessionAuthenticator` ([Sessão](./sessao.md)) |
| `Auth::user()` / `auth()->user()` | parâmetro `Principal $user`; a entidade de usuário você carrega pelo `identifier()` |
| `Password::sendResetLink`, verificação de e-mail | `PasswordResets`, `EmailVerification` e as rotas do `AuthRoutes` |
| Socialite | `Nimbus\Auth\OAuth\SocialLogin` (sobre `league/oauth2-client`) |
| `Hash::make()` / `Hash::check()` | `Nimbus\Security\Hash::make()` / `::check()` |
| `Crypt::encryptString()` | `Crypt::fromEnvironment()->encrypt()` |
| `URL::temporarySignedRoute()` | `UrlSigner::fromEnvironment()->temporary($path, $ttl)` |

### Gate e policies

::: code-group

```php [Laravel]
// AppServiceProvider::boot()
Gate::before(fn (User $user) => $user->is_admin ? true : null);
Gate::define('reports:export', fn (User $user) => $user->is_manager);
Gate::policy(Order::class, OrderPolicy::class);

// policy
public function cancel(User $user, Order $order): bool
{
    return $order->customer_id === $user->id;
}

// controller
$this->authorize('cancel', $order);
if (Gate::allows('reports:export')) { /* ... */ }

// rota
->middleware('can:cancel,order')
```

```php [Nimbus]
use Nimbus\Auth\Gate;
use Nimbus\Auth\Principal;

// bootstrap/app.php
$gate = $app->gate();
$gate->before(fn (Principal $user, string $ability) => $user->identifier() === 'user:1' ? true : null);
$gate->define('reports:export', fn (Principal $user) => $user->can('reports:*'));
$gate->policy(Order::class, OrderPolicy::class);

// policy: classe comum, montada pelo container
public function cancel(Principal $user, Order $order): bool
{
    return 'user:' . $order->customer->id === $user->identifier();
}

// controller
public function cancel(Order $order, Gate $gate): Response
{
    $gate->authorize('cancel', $order);   // 403, ou 401 sem principal
    if ($gate->allows('reports:export')) { /* ... */ }
}

// rota
->can('cancel', 'order')
```

:::

A policy recebe o `Principal`, não a entidade de usuário. Uma ability sem regra cai nas abilities do token, mas o escopo `*` sozinho não a libera ([Autorização](./autorizacao.md)).

## Filas

Um job que vai para a fila implementa `Nimbus\Jobs\SerializableJob`: nome estável e payload em array, sem `serialize()`. Veja [Filas e agendador](./filas.md).

::: code-group

```php [Laravel]
namespace App\Jobs;

use Illuminate\Contracts\Queue\ShouldQueue;
use Illuminate\Foundation\Queue\Queueable;

final class SendInvoice implements ShouldQueue
{
    use Queueable;

    public int $tries = 5;
    public array $backoff = [10, 60, 300];

    public function __construct(public int $invoiceId) {}

    public function handle(InvoiceMailer $mailer): void
    {
        $mailer->send($this->invoiceId);
    }

    public function failed(\Throwable $e): void { /* ... */ }
}

SendInvoice::dispatch($invoice->id)->delay(60);
// php artisan queue:work redis --tries=5
// php artisan queue:failed / queue:retry {id}
```

```php [Nimbus]
namespace App\Jobs;

use Nimbus\Jobs\SerializableJob;

final readonly class SendInvoice implements SerializableJob
{
    public function __construct(public int $invoiceId) {}

    public static function jobName(): string
    {
        return 'send-invoice';
    }

    public function toJobPayload(): array
    {
        return ['invoice_id' => $this->invoiceId];
    }

    public static function fromJobPayload(array $payload): self
    {
        return new self((int) $payload['invoice_id']);
    }

    public function handle(InvoiceMailer $mailer): void
    {
        $mailer->send($this->invoiceId);
    }
}

// action: $queue é o Nimbus\Jobs\JobDispatcher do container
$queue->dispatch(new SendInvoice($invoice->id), maximumAttempts: 5, delaySeconds: 60);
```

:::

Montagem da fila e do worker no boot:

```php
use Nimbus\Jobs\ExponentialBackoff;
use Nimbus\Jobs\InvokableJobHandler;
use Nimbus\Jobs\JobCodec;
use Nimbus\Jobs\JobDispatcher;
use Nimbus\Jobs\JobWorker;
use Nimbus\Jobs\RedisJobQueue;
use Nimbus\Redis\RedisPool;

$codec = new JobCodec([SendInvoice::class]);   // só as classes listadas são aceitas
$queue = new RedisJobQueue(RedisPool::fromEnvironment(), $codec, queue: 'default');
$app->container()->instance(JobDispatcher::class, $queue);

$app->background()->queue(new JobWorker(
    $queue,
    new InvokableJobHandler($app->container()),
    leaseSeconds: 60,
    retryPolicy: new ExponentialBackoff(baseSeconds: 5, maximumSeconds: 3_600),
));
```

| Laravel | Nimbus |
| --- | --- |
| `$tries` | `maximumAttempts:` no `dispatch()` |
| `$backoff` | `retryPolicy:` do `JobWorker` (`ExponentialBackoff`) |
| `->delay(60)` | `delaySeconds: 60` |
| `$this->release()` / `$this->fail()` | `handle()` devolve `JobOutcome::Retry` / `JobOutcome::DeadLetter` |
| driver `redis` / `database` | `RedisJobQueue` / `DatabaseJobQueue` (migration `QueueTablesMigration`) |
| `queue:work` + Supervisor | `pam start index.php --task-workers 1`, ou `$app->background()->run()` num processo à parte |
| `queue:failed`, `queue:retry`, `queue:forget` | `$queue->failed()`, `retryFailed($id)`, `forgetFailed($id)` |
| `sync` (testes) | `MemoryJobDispatcher` ou `Queue::fake()` |

A entrega é pelo menos uma vez: `handle()` precisa ser idempotente. A tabela de falhas guarda só a classe da exceção, nunca a mensagem.

## Eventos e broadcasting

Não há `event()` nem `Event::dispatch()`: o `Nimbus\Events\EventDispatcher` vem do container. Listeners são callables ou classes com `__invoke()`. Veja [Eventos e broadcasting](./eventos.md).

::: code-group

```php [Laravel]
// EventServiceProvider ou descoberta automática
Event::listen(OrderShipped::class, AuditOrder::class);
Event::listen(OrderShipped::class, SendShipmentEmail::class);

final class SendShipmentEmail implements ShouldQueue
{
    public int $tries = 5;

    public function handle(OrderShipped $event): void { /* ... */ }
}

OrderShipped::dispatch($order->id);
event(new OrderShipped($order->id));
```

```php [Nimbus]
use Nimbus\Events\EventDispatcher;
use Nimbus\Events\ShouldQueue;
use Nimbus\Events\SyncEventDispatcher;

// bootstrap/app.php
$events = (new SyncEventDispatcher($app->container()))
    ->listen(OrderShipped::class, AuditOrder::class)
    ->listen(OrderShipped::class, SendShipmentEmail::class);
$app->container()->instance(EventDispatcher::class, $events);

final class SendShipmentEmail implements ShouldQueue
{
    public int $tries = 5;

    public function __construct(private readonly Mailer $mailer) {}

    public function __invoke(OrderShipped $event): void { /* ... */ }
}

// action: EventDispatcher $events injetado
$events->dispatch(new OrderShipped($order->id));
```

:::

- Listener com `ShouldQueue` vira um job `Nimbus\Events\CallQueuedListener`: registre essa classe no `JobCodec` e tenha um `JobDispatcher` no container. `$tries` (padrão 3), `$delay` e `shouldQueue($event)` ajustam ou pulam o job.
- O evento precisa atravessar a fila: implemente `SerializableJob` ou use só propriedades públicas escalares, arrays e `null` aceitas pelo construtor.
- Não há descoberta automática de listeners nem `Event::subscribe()`.

### Broadcasting

::: code-group

```php [Laravel]
final class OrderStatusChanged implements ShouldBroadcast
{
    public function __construct(public int $orderId, public string $status) {}

    public function broadcastOn(): array
    {
        return [new PrivateChannel('orders.' . $this->orderId)];
    }

    public function broadcastAs(): string
    {
        return 'order.status';
    }
}

// routes/channels.php
Broadcast::channel('orders.{order}', fn (User $user, int $order) => $user->owns($order));
```

```php [Nimbus]
use Nimbus\Broadcasting\BroadcastManager;
use Nimbus\Broadcasting\PrivateChannel;
use Nimbus\Broadcasting\PusherBroadcaster;
use Nimbus\Broadcasting\ShouldBroadcast;

final readonly class OrderStatusChanged implements ShouldBroadcast
{
    public function __construct(public int $orderId, public string $status) {}

    public function broadcastOn(): array
    {
        return [new PrivateChannel('orders.' . $this->orderId)];
    }

    public function broadcastAs(): string
    {
        return 'order.status';
    }
}

// bootstrap/app.php
$broadcasts = new BroadcastManager(PusherBroadcaster::fromEnvironment());
$app->container()->instance(BroadcastManager::class, $broadcasts);
$broadcasts->channel('orders.{order}', fn (Principal $user, string $order) => $orders->ownedBy($user, (int) $order));
$broadcasts->routes($app);   // POST /broadcasting/auth para pusher-js / Laravel Echo
```

:::

O evento sai pelo `$events->dispatch()`; com `JobDispatcher` no container ele vai como job `BroadcastEvent`, e `ShouldBroadcastNow` transmite na hora. Drivers: `PusherBroadcaster`, `RedisBroadcaster` e `PamSocketBroadcaster`. Os callbacks de canal recebem o `Principal` e os segmentos como string.

## E-mail

Não há `Mailable`. O `Nimbus\Mail\Mailer` (sobre `symfony/mailer`) envia um `MailMessage`, o mesmo construtor das notificações, ou um `Symfony\Component\Mime\Email`.

::: code-group

```php [Laravel]
final class Welcome extends Mailable
{
    public function __construct(public User $user) {}

    public function envelope(): Envelope
    {
        return new Envelope(subject: 'Bem-vinda');
    }

    public function content(): Content
    {
        return new Content(markdown: 'mail.welcome');
    }
}

Mail::to($user->email)->send(new Welcome($user));
Mail::to($user->email)->queue(new Welcome($user));
```

```php [Nimbus]
use Nimbus\Mail\Mailer;
use Nimbus\Notifications\Mail\MailMessage;

// bootstrap/app.php: MAILER_DSN (ou MAIL_*) e MAIL_FROM_ADDRESS
$app->container()->instance(Mailer::class, Mailer::fromEnvironment());

// action ou job: Mailer $mailer injetado
$mailer->to([$user->email => $user->name])->send(
    MailMessage::make()
        ->subject('Bem-vinda')
        ->greeting("Olá, {$user->name}")
        ->line('Sua conta está pronta.')
        ->action('Entrar', 'https://app.example.com/login'),
);
```

:::

`MailMessage` tem `from()`, `replyTo()`, `cc()`, `bcc()`, `html()` e `text()`. Não há `Mail::queue()`: para enviar fora da requisição, mande um job que chama o `Mailer` ou use uma notificação com `sendLater()`.

## Notificações

::: code-group

```php [Laravel]
final class InvoicePaid extends Notification implements ShouldQueue
{
    public function via(object $notifiable): array
    {
        return ['mail', 'database'];
    }

    public function toMail(object $notifiable): MailMessage
    {
        return (new MailMessage)->subject('Fatura paga')->line('Recebemos o pagamento.');
    }

    public function toArray(object $notifiable): array
    {
        return ['invoice' => $this->invoice->id];
    }
}

$user->notify(new InvoicePaid($invoice));
Notification::send($users, new InvoicePaid($invoice));
```

```php [Nimbus]
use Nimbus\Notifications\Mail\MailMessage;
use Nimbus\Notifications\Notifiable;
use Nimbus\Notifications\Notification;

final class InvoicePaid extends Notification
{
    public function __construct(private readonly Invoice $invoice) {}

    public function via(Notifiable $notifiable): array
    {
        return ['mail', 'database'];
    }

    public function toMail(Notifiable $notifiable): MailMessage
    {
        return MailMessage::make()->subject('Fatura paga')->line('Recebemos o pagamento.');
    }

    public function toDatabase(Notifiable $notifiable): array
    {
        return ['invoice' => $this->invoice->id];
    }
}

// Notifier $notifier injetado; User implementa Notifiable (trait RoutesNotifications)
$notifier->send($user, new InvoicePaid($invoice));
$notifier->sendLater($users, new InvoicePaid($invoice));
```

:::

`toArray()` do canal `database` vira `toDatabase()`. Há canais `push` (FCM e APNs) e `webhook` assinado. `ShouldQueue` na notificação vira a escolha entre `send()` e `sendLater()`. Veja [Notificações](./notificacoes.md).

## Cache

A API do repositório segue o Laravel. Em serviços, injete `Nimbus\Cache\Repository`; a classe estática `Nimbus\Cache\Cache` usa o mesmo repositório. Veja [Cache da aplicação e tags](./cache.md).

::: code-group

```php [Laravel]
use Illuminate\Support\Facades\Cache;

$profile = Cache::remember("profile:{$id}", 600, fn () => $this->load($id));
Cache::put('rate', $rate, now()->addMinutes(5));
Cache::increment('visits');
Cache::forget("profile:{$id}");

Cache::tags(['users', "user:{$id}"])->remember("profile:{$id}", 600, fn () => $this->load($id));
Cache::tags(["user:{$id}"])->flush();
```

```php [Nimbus]
use Nimbus\Cache\Cache;
use Nimbus\Cache\Repository;
use Nimbus\Redis\RedisCacheStore;
use Nimbus\Redis\RedisPool;

// bootstrap/app.php
$cache = new Repository(new RedisCacheStore(RedisPool::fromEnvironment()));
$app->container()->instance(Repository::class, $cache);
Cache::useRepository($cache);

// serviço com Repository $cache injetado
$profile = $cache->remember("profile:{$id}", 600, fn () => $this->load($id));
$cache->put('rate', $rate, new \DateTimeImmutable('+5 minutes'));
$cache->increment('visits');
$cache->forget("profile:{$id}");

$cache->tags(['users', "user:{$id}"])->remember("profile:{$id}", 600, fn () => $this->load($id));
$cache->tags(["user:{$id}"])->flush();
```

:::

- O TTL é em segundos ou um `\DateTimeInterface`; `forever()`, `rememberForever()`, `add()`, `pull()` e `decrement()` existem.
- Sem `useRepository()`, o cache é um `MemoryStore` por worker: não é compartilhado entre workers.
- O `RedisCacheStore` só reconstrói objetos das classes em `allowedClasses`; prefira guardar arrays e escalares.
- Não há `Cache::lock()` nem `Cache::store('nome')`. Para cache de respostas HTTP inteiras, use `ResponseCacheMiddleware` ([Redis](./redis.md#cache-de-resposta)).

## Configuração e `.env`

Não há `config()` nem pasta `config/`. A configuração é declarada uma vez, com tipos, e o boot falha se faltar variável. Veja [Configuração tipada](./configuracao.md).

::: code-group

```php [Laravel]
// config/services.php
return [
    'stripe' => [
        'key' => env('STRIPE_KEY'),
        'timeout' => (int) env('STRIPE_TIMEOUT', 10),
        'sandbox' => (bool) env('STRIPE_SANDBOX', false),
    ],
];

// uso
$key = config('services.stripe.key');
```

```php [Nimbus]
use Nimbus\Config\ConfigDefinition;
use Nimbus\Config\ConfigType;
use Nimbus\Config\Configuration;

// bootstrap/app.php
$config = Configuration::fromEnvironment([
    new ConfigDefinition('stripe.key', 'STRIPE_KEY', sensitive: true),
    new ConfigDefinition('stripe.timeout', 'STRIPE_TIMEOUT', ConfigType::Integer, required: false, default: 10),
    new ConfigDefinition('stripe.sandbox', 'STRIPE_SANDBOX', ConfigType::Boolean, required: false, default: false),
]);
$app->container()->instance(Configuration::class, $config);

// uso, com Configuration $config injetado
$key = $config->string('stripe.key');
$timeout = $config->integer('stripe.timeout');
```

:::

- Os módulos leem as próprias variáveis (`DatabaseConfig::fromEnvironment()`, `RedisPool::fromEnvironment()`, `Mailer::fromEnvironment()`), com os mesmos nomes do Laravel quando existem (`DB_*`, `REDIS_*`, `MAIL_*`).
- O `.env` é lido pelo `bootstrap/app.php` do template, e variáveis já definidas no processo ganham do arquivo. Em produção, o `pam deploy:remote` liga `shared/.env` e passa ao servidor com `--env-file`.
- `$config->redacted()` mascara os valores marcados como `sensitive` para diagnóstico.
- Não há `config:cache` nem `config(['x' => 'y'])` em tempo de execução.

## Cliente HTTP

`Nimbus\Http\Client\Http` tem a API do `Http` do Laravel. Dentro do PAM a chamada suspende só a Fiber da requisição; fora dele usa o `symfony/http-client`.

::: code-group

```php [Laravel]
use Illuminate\Support\Facades\Http;

$id = Http::withToken($token)
    ->timeout(5)
    ->retry(3, 100)
    ->post('https://erp.example.com/orders', ['sku' => 'A1'])
    ->throw()
    ->json('data.id');

$responses = Http::pool(fn (Pool $pool) => [
    $pool->as('users')->get('https://api.example.com/users'),
    $pool->as('orders')->get('https://api.example.com/orders'),
]);
```

```php [Nimbus]
use Nimbus\Http\Client\Http;
use Nimbus\Http\Client\Pool;

$id = Http::withToken($token)
    ->timeout(5)
    ->retry(3, 100)
    ->post('https://erp.example.com/orders', ['sku' => 'A1'])
    ->throw()
    ->json('data.id');

$responses = Http::pool(fn (Pool $pool) => [
    $pool->as('users')->get('https://api.example.com/users'),
    $pool->as('orders')->get('https://api.example.com/orders'),
]);
```

:::

`baseUrl()`, `withHeaders()`, `withBasicAuth()`, `acceptJson()`, `asForm()`, `withQueryParameters()` e `withoutRedirecting()` existem. Não há macros nem middleware de cliente (`withMiddleware()`).

## Agendador

O agendador roda dentro do PAM, nos task workers, sem cron do sistema. Veja [Filas e agendador](./filas.md#agendador).

::: code-group

```php [Laravel]
// routes/console.php
Schedule::job(new SendReport)->hourly();
Schedule::call(fn () => Cart::pruneAbandoned())
    ->everyFiveMinutes()
    ->withoutOverlapping()
    ->onOneServer();
Schedule::command('reports:monthly')->monthlyOn(1, '06:00');

// crontab: * * * * * php artisan schedule:run
```

```php [Nimbus]
use Nimbus\Redis\RedisPool;
use Nimbus\Scheduling\RedisSchedulerLock;
use Nimbus\Scheduling\Schedule;
use Nimbus\Scheduling\Scheduler;

// bootstrap/app.php
$schedule = new Schedule($queue, timezone: 'America/Sao_Paulo', container: $app->container());

$schedule->job(SendReport::class)->hourly();
$schedule->call(fn () => $app->container()->get(CartService::class)->pruneAbandoned())
    ->everyFiveMinutes()
    ->withoutOverlapping()
    ->name('prune-carts');

$app->background()->scheduler(new Scheduler($schedule, new RedisSchedulerLock(RedisPool::fromEnvironment())));

// pam start index.php --workers 4 --task-workers 1
```

:::

- `onOneServer()` não existe como método: com um `SchedulerLock` compartilhado (`RedisSchedulerLock` ou `DatabaseSchedulerLock`), cada tarefa já roda uma vez por minuto em todo o cluster.
- Não há `Schedule::command()`: agende um job ou uma closure.
- `job()` monta o job pelo container; a closure de `call()` não recebe parâmetros injetados, então busque o serviço no container dentro dela.
- Requer `dragonmantank/cron-expression`.

## Console e tinker

| Laravel | Nimbus |
| --- | --- |
| `php artisan tinker` | `bin/nimbus tinker` (requer `psy/psysh`), com `$app`, `$container` e, com Arcano, `$arcano` e `$orm` no escopo |
| `php artisan tinker --execute='...'` | `bin/nimbus tinker --execute='...'` |
| `php artisan serve` | `pam dev index.php` |
| `php artisan route:list` | `pam routes` |
| `php artisan test` | `pam test` |
| `php artisan make:*` | `pam make:controller`, `make:model`, `make:request`, `make:resource`, `make:repository`, `make:job`, `make:factory`, `make:seeder`, `make:migration`, `make:test` |
| comandos próprios (`Command`, `$signature`) | sem classe de comando no Nimbus: escreva um script em `bin/` que carrega `bootstrap/app.php` (como o `bin/migrate`) e rode com `pam exec bin/x` |
| `composer require` | `pam composer require` |

O `bin/nimbus tinker` carrega o mesmo `bootstrap/app.php` do `index.php` e dos testes; `--bootstrap=caminho` aponta outro arquivo.

## Testes

`Nimbus\Testing\TestCase` monta uma aplicação nova por teste e envia as requisições em memória. Os nomes das asserções seguem o Laravel. Veja [Testes](./testes.md).

::: code-group

```php [Laravel]
namespace Tests\Feature;

use App\Events\OrderShipped;
use App\Jobs\SendInvoice;
use App\Models\User;
use Illuminate\Foundation\Testing\RefreshDatabase;
use Illuminate\Support\Facades\Event;
use Illuminate\Support\Facades\Http;
use Illuminate\Support\Facades\Mail;
use Illuminate\Support\Facades\Queue;
use Laravel\Sanctum\Sanctum;
use Tests\TestCase;

final class OrderTest extends TestCase
{
    use RefreshDatabase;

    public function test_ships_an_order(): void
    {
        Queue::fake();
        Event::fake();
        Mail::fake();
        Http::fake(['erp.example.com/*' => Http::response(['status' => 'synced'])]);
        Sanctum::actingAs(User::factory()->create(), ['orders:write']);

        $this->postJson('/v1/orders/12/ship')
            ->assertOk()
            ->assertJsonPath('data.status', 'shipped');

        Queue::assertPushed(SendInvoice::class, fn ($job) => $job->invoiceId === 12);
        Event::assertDispatched(OrderShipped::class);
        Mail::assertSent(ShipmentMail::class);
        Http::assertSent(fn ($request) => $request['id'] === 12);
        $this->assertDatabaseHas('orders', ['id' => 12, 'status' => 'shipped']);
    }
}
```

```php [Nimbus]
namespace Tests\Feature;

use App\Events\OrderShipped;
use App\Jobs\SendInvoice;
use Nimbus\Auth\TokenPrincipal;
use Nimbus\Http\Client\OutgoingRequest;
use Nimbus\Testing\Event;
use Nimbus\Testing\Mail;
use Nimbus\Testing\Queue;
use Nimbus\Testing\RefreshDatabase;
use Tests\TestCase;

final class OrderTest extends TestCase
{
    use RefreshDatabase;

    public function test_ships_an_order(): void
    {
        Queue::fake();
        Event::fake();
        Mail::fake();
        $http = $this->fakeHttp(['erp.example.com/*' => ['status' => 'synced']]);

        $this->actingAs(new TokenPrincipal('user:1', ['orders:write'], 'test'))
            ->postJson('/v1/orders/12/ship')
            ->assertOk()
            ->assertJsonPath('data.status', 'shipped');

        Queue::assertPushed(SendInvoice::class, fn (SendInvoice $job) => $job->invoiceId === 12);
        Event::assertDispatched(OrderShipped::class);
        Mail::assertSentTo('ana@example.com', 'Pedido enviado');
        $http->assertSent(fn (OutgoingRequest $request) => $request->data()['id'] === 12);
        $this->assertDatabaseHas('orders', ['id' => 12, 'status' => 'shipped']);
    }
}
```

:::

- `Tests\TestCase` estende `Nimbus\Testing\TestCase` e implementa `createApplication()` (normalmente `require` do `bootstrap/app.php`).
- `actingAs()` recebe um `Principal`, não um model.
- Os fakes (`Queue`, `Event`, `Mail`, `Storage`, `Broadcast`) ficam em `Nimbus\Testing` e trocam o serviço no container da aplicação de teste; chame `fake()` antes que o serviço seja resolvido. Também há `$this->fakeQueue()`, `fakeEvents()`, `fakeMail()`, `fakeStorage()`, `fakeBroadcasts()` e `fakeHttp()`.
- `Mail::assertSent()` recebe uma closure sobre o `Symfony\Component\Mime\Email` (não há classe `Mailable` para comparar); `assertSentTo($endereco, $assunto)` cobre o caso comum.
- `Http::fake()` devolve o `HttpFake`, e as asserções (`assertSent`, `assertNotSent`, `assertSentCount`, `assertNothingSent`) ficam nele, não na classe estática.
- `Notification::fake()` vira `Notifier::fake()`, registrado como `NotificationDispatcher` ([Notificações](./notificacoes.md#testes)).
- `RefreshDatabase` roda as migrations pendentes e envolve o teste numa transação com rollback. Use SQLite em memória no `phpunit.xml`.
- `assertDatabaseHas`, `assertDatabaseMissing`, `assertDatabaseCount` e `$this->seed()` existem com os mesmos nomes.

## Workers persistentes

No Laravel com PHP-FPM, cada requisição começa do zero. No Nimbus, o worker do PAM carrega o app uma vez e atende milhares de requisições. O que muda no código:

| Assunto | Laravel (FPM) | Nimbus |
| --- | --- | --- |
| Boot | providers rodam a cada requisição | `bootstrap/app.php` roda uma vez por worker; depois o app congela e não aceita rota, provider ou middleware novos |
| `singleton` | uma instância por requisição, na prática | uma instância por worker, compartilhada por todas as requisições; não pode guardar usuário, tenant ou dados da requisição |
| Estado da requisição | `singleton`, `app()->instance()` ou propriedade estática funcionam | use `scoped()` (uma instância por requisição, descartada no fim mesmo com exceção) ou passe pelo `Request` |
| Propriedades estáticas | zeradas a cada requisição | sobrevivem entre requisições; um array estático que só cresce é vazamento de memória |
| Usuário autenticado | `Auth::user()` global | `Principal` no escopo da requisição, recebido como parâmetro |
| Conexões de banco e Redis | abertas por requisição | pool por worker (`maxIdleConnections`, `maxIdle`); transações abertas sofrem rollback no fim da requisição |
| Identity map do ORM | não existe no Eloquent | um por requisição (Fiber), sem vazar entidades entre usuários |
| `exit()`, `die()`, `header()`, `echo` | encerram ou escrevem na resposta | não use: devolva uma `Response` |
| Concorrência | uma requisição por processo | várias requisições por worker em Fibers; I/O do PAM (HTTP, Redis, Postgres com `DB_ASYNC`) suspende só a Fiber |
| Reflexão | por requisição | o plano de parâmetros do container e o schema dos `Input` são compilados uma vez por worker |
| Trabalho pós-resposta | `terminate()`, `dispatch()->afterResponse()` | não existe; mande um job para a fila |
| Deploy | troca de arquivos basta | o código fica em memória: reinicie os workers (`pam deploy:remote` faz isso) |

## Sem equivalente por enquanto

- **Fachadas em geral** (`DB::`, `Auth::`, `Route::`, `Event::`, `Log::`): use injeção. Só `Http`, `Cache` e `Hash` têm classes estáticas.
- **Validação**: `messages()` e `attributes()` do FormRequest, mensagens traduzidas por regra, regras com banco (`unique`, `exists`), regras condicionais (`required_if`, `sometimes`, `exclude_*`), regras com ponto e curinga (`items.*.sku`; use `Input` aninhado ou listas tipadas), `after()` e `prepareForValidation()`.
- **Request**: `merge()`, `user()`, `file()` e `UploadedFile` (`input()`, `all()`, `only()` e `bearerToken()` existem como `Nimbus\Http\RequestData`). O `multipart/form-data` só entrega os campos de texto; para arquivos, use upload direto com `$disk->temporaryUploadUrl()` ([Storage](./storage.md)).
- **Rotas**: helper `route('nome')` para gerar URL, prefixo de nome em grupo, aliases e grupos de middleware por nome, `Route::fallback()`, rotas `create`/`edit` no `resource()`, binding implícito sem `bindRoute()`.
- **Middleware**: `terminate()` e trabalho depois do envio da resposta.
- **Eloquent**: soft deletes, eventos e observers de model, `$fillable`/mass assignment, `firstOrCreate`/`updateOrCreate`, `firstOrFail`, relações polimórficas no estilo `morphTo` com a mesma API, `chunk()`/`lazy()` com a mesma API, `withCount()`, casts customizados de model.
- **Filas**: método `failed()` no job, `Bus::chain()`, `Bus::batch()`, `ShouldBeUnique`, middleware de job (`RateLimited`, `WithoutOverlapping`), `dispatch()->afterResponse()`, `dispatchSync()`, filas por conexão no mesmo dispatcher (crie uma fila por `queue:`), Horizon.
- **Eventos**: descoberta automática de listeners, `Event::subscribe()`, `$event->until()`.
- **E-mail**: `Mailable`, templates Markdown de e-mail, `Mail::queue()`, anexos pela API do Nimbus (use um `Symfony\Component\Mime\Email` direto).
- **Cache**: `Cache::lock()`, vários stores nomeados (`Cache::store()`), `flexible()`.
- **Configuração**: `config()` global, `config:cache`, alterar configuração em tempo de execução.
- **Agendador**: `Schedule::command()`, `->onOneServer()` como método (o lock compartilhado já cobre), `->runInBackground()`, saída para arquivo.
- **Console**: classes de comando Artisan; só existe `bin/nimbus tinker`.
- **Autenticação**: guards nomeados, `Auth::user()` devolvendo a entidade, `Auth::attempt()` (confira a senha com `Hash::check()` e emita o token), `RateLimiter::attempt()` fora do middleware.
- **Testes**: `Sanctum::actingAs($user)` com model, `Mail::assertSent(Classe::class)`, `Http::assertSent()` estático, `$this->travel()`/`freezeTime()`, `withoutMiddleware()`, `Bus::fake()`.
- **Outros**: Livewire, Inertia, Horizon, Telescope, Pulse, Cashier e Scout.

## Referência rápida

- `routes/api.php` e `routes/web.php` → `routes.php`
- `app/Http/Controllers` → `app/Controllers`
- `app/Models` → `app/Models` (entidades Arcano)
- `bootstrap/app.php` → `bootstrap/app.php` (monta o `Nimbus\App`)
- `HandleCors` → `CorsMiddleware`
- Scribe / L5-Swagger → `$app->docs()` e `$app->openApi()` ([OpenAPI](./openapi.md))
- `Storage::disk('s3')->put()` → `$app->storage()->disk('s3')->put()` ou `Disk` injetado; `temporaryUrl()` igual
- `Redis::command()` → `RedisPool::command([...])`
- Blade (`.blade.php`) → views `.nim.php` com as diretivas principais e componentes `<x-...>` ([Views](./views.md))
- `session()->flash()` → `$session->flash()` com `Session` injetada
- `@csrf`, `VerifyCsrfToken` → `@csrf`, `VerifyCsrfTokenMiddleware`
- `__('x')`, `trans_choice()` → `__('x')`, `trans_choice()` com `lang/{locale}` ([Tradução](./traducao.md))
- Laravel Forge / Envoyer → `pam deploy:remote` ([Deploy](./deploy.md))

## Ordem sugerida

1. `pam init app --template api` ao lado do app atual.
2. Rodar o Rector com `--dry-run` em cópias de `app/` e `routes/` para medir o que sai automático.
3. Portar as entidades para Arcano e gerar as migrations pela diferença; conferir com `status()` contra uma cópia do banco.
4. Portar rotas e controllers por recurso, com os testes de feature junto.
5. Trocar autenticação, filas e eventos.
6. Revisar `singleton` e propriedades estáticas pela tabela de [workers persistentes](#workers-persistentes).
7. Medir os endpoints mais usados antes de trocar o tráfego.
