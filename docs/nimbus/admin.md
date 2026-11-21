# Painel administrativo

`Nimbus\Admin` gera telas de listagem, criação, edição e exclusão para entidades do Arcano. É opcional: só existe no app que chama `Admin::panel()`. As telas são templates `.nim.php` renderizados no servidor, com CSS próprio (tema claro e escuro, sem dependência de JavaScript) e um script de 2 KB que só adiciona o alternador de tema, o "selecionar todos" e a confirmação de exclusão.

## Requisitos

- `ArcanoServiceProvider` (as entidades e o schema compilado).
- `SessionServiceProvider` (sessão, flash e CSRF).
- Um `Auth\Authenticator` no container, normalmente o `SessionAuthenticator` da tela de login do app.

## Registrar

```php
use App\Admin\ProductAdmin;
use App\Models\Category;
use App\Models\User;
use Nimbus\Admin\Admin;

Admin::panel($app, '/admin', loginUrl: '/login', title: 'Loja')
    ->locale('pt_BR')
    ->resource(ProductAdmin::class)  // subclasse de AdminResource
    ->resource(Category::class);     // ou só a entidade, com os padrões

$app->gate()->define('viewAdmin', static fn (User $user): bool => $user->isAdmin);
```

Rotas criadas sob o prefixo:

| Método | Caminho | Ação |
| --- | --- | --- |
| GET | `/admin` | painel com a contagem de cada recurso |
| GET | `/admin/{recurso}` | lista: `q`, `f[campo]`, `sort`, `dir`, `page` |
| GET | `/admin/{recurso}/create` | formulário de criação |
| POST | `/admin/{recurso}` | grava o novo registro |
| GET | `/admin/{recurso}/{id}/edit` | formulário de edição |
| PUT | `/admin/{recurso}/{id}` | grava a edição (`_method=PUT`) |
| DELETE | `/admin/{recurso}/{id}` | exclui (`_method=DELETE`) |
| POST | `/admin/{recurso}/bulk` | ação em massa (`action`, `ids[]`) |

Visitante sem login vai para `loginUrl?redirect=/admin/...`; sem `loginUrl`, recebe 401. O slug do recurso vem do nome da classe no plural (`BlogPost` vira `blog-posts`).

## Campos a partir do Arcano

O painel lê o schema compilado e os atributos `#[Column]` da entidade:

| Coluna | Campo |
| --- | --- |
| `primary`, `bigPrimary` | `id` (só na lista) |
| `string(n)`, `char` | `text` com `maxlength` n; `email`/`url` pelo nome da propriedade |
| `text`, `longText` | `textarea` |
| `integer`, `bigInteger` | `number` |
| `float`, `decimal(p,s)` | `decimal` |
| `boolean` | `boolean` (checkbox) |
| `date`, `datetime`, `time` | campos de data e hora |
| `enum(a,b)` ou propriedade `BackedEnum` | `select` |
| `json` | `json` (textarea validado) |
| `#[BelongsTo]` | `belongsTo` (select com os registros do alvo) |

Coluna `nullable` aceita vazio; coluna sem `default` e não nula é obrigatória. Propriedades `password*`, `remember_token` e `secret` ficam fora da lista e do formulário. Os atributos de validação do Nimbus na entidade (`#[Max]`, `#[Min]`, `#[Email]`, `#[In]`...) viram regras do campo.

## Personalizar um recurso

```php
namespace App\Admin;

use App\Models\Product;
use Arcano\ORM\Select;
use Nimbus\Admin\AdminAction;
use Nimbus\Admin\AdminField;
use Nimbus\Admin\AdminResource;
use Nimbus\Database\ArcanoManager;
use Nimbus\Validation\Attributes\Min;
use Pam\Http\Request;

final class ProductAdmin extends AdminResource
{
    protected string $entity = Product::class;

    public function fields(array $fields): array
    {
        $fields['price']->label('Preço (centavos)')->rules(new Min(0));
        $fields['description']->help('Aparece na página do produto.');
        $fields['sku']->searchable()->readonly();
        $fields['status'] = AdminField::select('status', ProductStatus::class)->filterable();
        unset($fields['internal_notes']);
        return $fields;
    }

    public function perPage(): int
    {
        return 50;
    }

    public function defaultSort(): ?array
    {
        return ['name', 'asc'];
    }

    public function title(object $entity): string
    {
        return $entity->name . ' (' . $entity->sku . ')';
    }

    public function query(Select $select, Request $request): Select
    {
        return $select->where('archived', false);
    }

    public function actions(): array
    {
        return [
            'archive' => new AdminAction('Arquivar', static function (array $products, ArcanoManager $arcano): string {
                $manager = $arcano->entityManager();
                foreach ($products as $product) {
                    $product->archived = true;
                    $manager->persist($product);
                }
                $manager->run();
                return count($products) . ' produtos arquivados.';
            }),
        ];
    }
}
```

`pam make:admin Product` cria esse arquivo em `app/Admin/ProductAdmin.php`.

Outros pontos de extensão:

- `input()`: devolve uma classe `Input` que valida o formulário no lugar das regras dos campos; as propriedades com o nome dos campos são copiadas para a entidade.
- `saving($entity, $creating)`: roda depois de os campos serem escritos e antes de persistir (gerar slug, preencher autor).
- `AdminField::format(fn ($value, $entity) => ...)`: texto da célula na lista.
- `onList()`, `onForm()`, `hidden()`, `sortable()`, `searchable()`, `filterable()`, `nullable()`, `required()`, `maxLength()`.

A busca usa `LIKE` (no Postgres, `ILIKE`) nos campos `searchable`; para tabelas grandes, crie índices nessas colunas ou restrinja a busca a uma coluna indexada.

## Autorização

1. Toda rota exige a ability `viewAdmin` no `Gate`, e ela precisa estar definida com `define('viewAdmin', ...)` (ou decidida por um hook `before()`). Sem isso o painel responde 403 a todos, inclusive a tokens com escopo `*`.
2. Se a entidade tem policy (`$app->gate()->policy(Product::class, ProductPolicy::class)`) ou o Gate define a ability, o painel verifica `viewAny` (lista e menu), `create`, `update` e `delete` contra a entidade. Sem policy, `viewAdmin` basta. Uma policy sem o método da ability nega.

As ações em massa conferem a ability de cada registro selecionado (`delete` para excluir, `update` ou a ability da `AdminAction` para as demais).

## Segurança

- Formulários levam `@csrf`; sem token válido a resposta é 419.
- As respostas do painel saem com `Cache-Control: no-store`, `X-Frame-Options: DENY` e uma CSP que só permite recursos do próprio app.
- Erros 403 e 404 viram páginas do painel para navegadores e problem+json para clientes JSON.
- Ids de rota e de ação em massa são limitados (64 caracteres, 1.000 registros por ação).

## Textos e templates

`->locale('pt_BR')` troca os textos embutidos (padrão `en`); `->messages(['save' => 'Gravar'])` substitui chaves avulsas. Para trocar um template, passe `views:` com um diretório: um arquivo com o mesmo nome (`layout.nim.php`, `index.nim.php`, `form.nim.php`, `field.nim.php`, `dashboard.nim.php`, `error.nim.php`) tem prioridade sobre o do pacote.

```php
Admin::panel($app, '/admin', views: __DIR__ . '/resources/views/admin');
```

## Testar

```php
public function test_admin_creates_products(): void
{
    $this->actingAs($admin);
    $page = $this->get('/admin/products/create')->assertOk();
    preg_match('/name="_token" value="([^"]+)"/', $page->body(), $token);

    $this->client()->withCookiesFrom($page)
        ->post('/admin/products', ['_token' => $token[1], 'name' => 'Caneca', 'price' => '2500'])
        ->assertRedirect('/admin/products');
    $this->assertDatabaseHas('products', ['name' => 'Caneca']);
}
```
