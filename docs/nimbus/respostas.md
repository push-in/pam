# Recursos JSON e paginação

## Resposta direta

O handler recebe `Pam\Http\Response` e pode responder com ela:

```php
$router->get('health', fn (Request $request, Response $response) => $response->json(['ok' => true]));
$router->post('events', fn (Request $request, Response $response) => $response->json(['queued' => true], 202));
```

## `JsonResource`

Um recurso transforma um objeto (normalmente uma entidade Arcano) no JSON da API e embrulha o resultado em `data`:

```php
namespace App\Http\Resources;

use Nimbus\Http\JsonResource;
use Pam\Http\Request;

/** @property-read \App\Models\Product $resource */
final readonly class ProductResource extends JsonResource
{
    public function toArray(Request $request): array
    {
        return [
            'id' => $this->resource->id,
            'name' => $this->resource->name,
            'price_in_cents' => $this->resource->priceInCents,
        ];
    }
}
```

```php
return new ProductResource($product);        // 200 {"data": {...}}
return new ProductResource($product, 201);   // status no segundo argumento
```

`ProductResource::collection($itens)` devolve uma `ResourceCollection` com a lista em `data`. `withMeta(['total' => 3])` define o membro `meta` (substitui o da paginação, se houver).

`JsonResource`, `ResourceCollection`, `View` e `Redirect` implementam `Nimbus\Http\Responsable`; qualquer classe sua que implemente `toResponse(Request, Response): Response` pode ser devolvida por um handler.

## Paginação

`Nimbus\Pagination\Paginator` pagina um `Select` do Arcano a partir da query string da requisição.

### Páginas numeradas

```php
use Nimbus\Pagination\Paginator;

public function index(Request $request): ResourceCollection
{
    $query = $this->arcano->select(Product::class)->orderBy('id');

    return ProductResource::collection(Paginator::paginate($query, $request, perPage: 20));
}
```

Lê `?page=` e `?per_page=` (máximo `Paginator::MAX_PER_PAGE`, 100). Faz um `COUNT` e uma consulta com `LIMIT/OFFSET`:

```json
{
  "data": [{"id": 1, "name": "Teclado", "price_in_cents": 25000}],
  "meta": {"current_page": 2, "per_page": 20, "last_page": 5, "total": 93},
  "links": {
    "first": "/products?page=1",
    "prev": "/products?page=1",
    "next": "/products?page=3",
    "last": "/products?page=5"
  }
}
```

### Cursor (keyset)

Para tabelas grandes, `Paginator::cursor()` evita o `OFFSET` e o `COUNT`: ordena por uma coluna única e busca os registros depois do último visto.

```php
return ProductResource::collection(Paginator::cursor($query, $request, perPage: 50));
// coluna e propriedade: Paginator::cursor($query, $request, 50, column: 'code', property: 'code')
```

Lê `?cursor=` e `?per_page=`. A resposta traz `meta.next_cursor` e `links.next`, ambos `null` na última página. O cursor é opaco para o cliente.

Os dois métodos devolvem um `Nimbus\Pagination\Page` (`items`, `meta`, `links`), que também pode ser usado sem recurso.
