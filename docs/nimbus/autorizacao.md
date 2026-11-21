# Autorização: Gate e policies

`Nimbus\Auth\Gate` (`$app->gate()`) decide se o principal autenticado pode executar uma ability. As regras são, em ordem:

1. hooks `before()`: devolvem `true` (permite), `false` (nega) ou `null` (segue);
2. policy da classe da entidade, um método por ability;
3. closure definida com `define()`;
4. sem regra, o `can()` do próprio principal, ou seja, as abilities do token. O escopo `*` sozinho
   (padrão dos tokens emitidos) não vale aqui: uma ability sem regra exige escopo explícito
   (`reports:export`) ou de namespace (`reports:*`). `$gate->wildcardScopesGrantUndefinedAbilities()`
   volta ao comportamento anterior.

```php
use Nimbus\Auth\Principal;

$gate = $app->gate();

$gate->before(fn (Principal $user, string $ability) => $user->identifier() === 'user:1' ? true : null);
$gate->define('reports:export', fn (Principal $user) => $user->can('reports:*'));
$gate->policy(Product::class, ProductPolicy::class);
```

## Policies

Uma policy é uma classe comum, montada pelo container. O nome do método vem da ability em camelCase (`view-any` chama `viewAny`):

```php
namespace App\Policies;

use App\Models\Product;
use Nimbus\Auth\Principal;

final readonly class ProductPolicy
{
    public function viewAny(Principal $user): bool
    {
        return true;
    }

    public function update(Principal $user, Product $product): bool
    {
        return $product->ownerId === $user->identifier() && $user->can('products:write');
    }
}
```

Uma policy pode ter seu próprio `before()`. Como a ability sem regra cai nas abilities do token, uma policy também pode exigir um escopo (`$user->can('products:write')`).

## Nas rotas

```php
$router->put('products/{product}', [ProductController::class, 'update'])->can('update', 'product');
$router->get('products', [ProductController::class, 'index'])->can('view-any', Product::class);
$router->post('products', [ProductController::class, 'store'])->can('products:write');

$admin = $router->prefix('admin')->can('admin');
```

- `can()` implica `auth()`.
- `can('update', 'product')` carrega o parâmetro de rota `product` pelo `bindRoute()` ou `RouteBindable` ([Controllers](./controllers.md#parametros-de-rota)) e passa a entidade para a policy.
- `can('view-any', Product::class)` chama a policy da classe sem instância.
- Sem principal a resposta é `401`; com principal sem permissão, `403` (`Forbidden`).

## No código

```php
use Nimbus\Auth\Gate;

public function update(Product $product, Gate $gate, UpdateProductRequest $input): ProductResource
{
    $gate->authorize('update', $product);   // lança 403 (401 sem principal)
    // ...
}

if ($gate->allows('reports:export')) { /* ... */ }
if ($gate->denies('update', $product)) { /* ... */ }
```

Nas views, `@can('update', $product)` e `@cannot` usam o mesmo Gate ([Views](./views.md)).
