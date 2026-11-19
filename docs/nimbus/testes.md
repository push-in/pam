# Testes

```bash
pam test
```

O `pam test` roda o PHPUnit (ou Pest) dentro do PAM. `Nimbus\Testing\TestCase` monta e inicializa uma aplicação nova por teste e manda as requisições em memória, sem abrir porta.

## TestCase

```php
namespace Tests;

use Nimbus\App;

abstract class TestCase extends \Nimbus\Testing\TestCase
{
    protected function createApplication(): App
    {
        return require dirname(__DIR__) . '/bootstrap/app.php';
    }
}
```

Sem `bootstrap/app.php` (template api), monte a aplicação no próprio método:

```php
protected function createApplication(): App
{
    $app = new App(discoverPackages: false);
    $app->provider(new ArcanoServiceProvider($config, $entities, migrations: $migrations));
    $app->routes(dirname(__DIR__) . '/routes.php');
    return $app;
}
```

No `phpunit.xml`, aponte o banco para SQLite em memória (`DB_CONNECTION=sqlite`, `DB_DATABASE=:memory:`), como faz o template web.

## Um teste de API

```php
namespace Tests\Feature;

use App\Models\Product;
use Database\Factories\ProductFactory;
use Nimbus\Testing\RefreshDatabase;
use Tests\TestCase;

final class ProductTest extends TestCase
{
    use RefreshDatabase;

    public function test_lists_products(): void
    {
        ProductFactory::new()->count(3)->sequence(['name' => 'Teclado'], ['name' => 'Mouse'])->create();

        $this->getJson('/products')
            ->assertOk()
            ->assertJsonCount(3, 'data')
            ->assertJsonPath('data.0.name', 'Teclado')
            ->assertJsonStructure(['data' => ['*' => ['id', 'name']]]);
    }

    public function test_validates_the_name(): void
    {
        $this->actingAs($user)
            ->postJson('/products', ['name' => ''])
            ->assertUnprocessableEntity()
            ->assertJsonValidationErrors(['name' => 'required']);

        $this->assertDatabaseCount('products', 0);
    }
}
```

### Requisições

- `get`, `getJson`, `postJson`, `putJson`, `patchJson`, `deleteJson` e `json($method, $path, $payload)`.
- `withHeaders()`, `withHeader()` e `withToken($token)` valem para as requisições seguintes.
- `actingAs($principal)` registra um `Authenticator` que devolve o principal, então rotas com `auth()`/`can()` o enxergam.
- `seed(DatabaseSeeder::class)` roda um seeder.
- `arcano()` devolve o `ArcanoManager` da aplicação de teste.

### Asserções da resposta (`TestResponse`)

| Status | Conteúdo |
| --- | --- |
| `assertOk`, `assertCreated`, `assertNoContent`, `assertSuccessful` | `assertJson` (subconjunto), `assertExactJson` |
| `assertStatus($code)` | `assertJsonPath($path, $valor ou closure)`, `assertJsonMissingPath` |
| `assertNotFound`, `assertUnauthorized`, `assertForbidden` | `assertJsonCount($n, $path)`, `assertJsonStructure` (`*` para listas) |
| `assertUnprocessableEntity` | `assertJsonValidationErrors`, `assertJsonMissingValidationErrors` |
| `assertRedirect($to)` | `assertHeader`, `assertHeaderMissing`, `assertSee`, `assertDontSee` |

`json($path)` lê um valor do corpo e `dump()` imprime a resposta. Falhas são falhas de asserção do PHPUnit.

### Banco

- `RefreshDatabase` roda as migrations pendentes e abre uma transação que todas as requisições do teste compartilham e que sofre rollback no fim. Uma requisição que deixa a própria transação aberta perde só as suas escritas, como em produção.
- `assertDatabaseHas`, `assertDatabaseMissing` e `assertDatabaseCount` recebem o nome da tabela (ou a classe da entidade) e as colunas.

## TestClient sem a classe base

`Nimbus\Testing\TestClient` oferece as mesmas requisições sobre qualquer `App`:

```php
use Nimbus\Testing\TestClient;

(new TestClient($app))
    ->withToken($token)
    ->postJson('/login', ['email' => 'dev@pam.dev'])
    ->assertSuccessful()
    ->assertJsonPath('data.status', 1);
```

## Páginas web

Para formulários, `TestClient` tem `post()` e `form()` (urlencoded), `withCookies()` e `withCookiesFrom($response)`; `TestResponse` tem `cookies()` e `cookie($nome)`. O template web traz um `tests/Browser.php` que guarda o cookie de sessão e manda o token CSRF em todo formulário:

```php
$browser = $this->browser();

$browser->post('/login', ['email' => 'ana@example.com', 'password' => 'password'])
    ->assertRedirect('/dashboard');
$browser->get('/dashboard')->assertOk()->assertSee('Ana Souza');
```

## Notificações e filas

- `Notifier::fake()` grava as notificações em vez de enviar ([Notificações](./notificacoes.md#testes)).
- `MemoryJobDispatcher` substitui a fila; `JobWorker::runOne()` processa um job no teste.
