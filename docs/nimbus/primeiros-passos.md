# Primeiros passos

Esta página cria uma API com o template `api`, roda o servidor de desenvolvimento, chama a rota de exemplo, roda o teste que vem pronto e adiciona a primeira entidade do Arcano com migration. A [Instalação e estrutura](./instalacao.md) descreve os templates e as pastas em detalhe.

## Instalar o PAM

```bash
curl -fsSL https://push-in.github.io/pam/install.sh | sh
pam doctor
```

O PAM embute o PHP 8.5 e o Composer (`pam composer ...`), então não é preciso ter PHP instalado na máquina.

## Criar o projeto

```bash
pam init minha-api --template api
cd minha-api
```

O `pam init` grava os arquivos e roda `pam composer install`. Com `--no-install`, só grava os arquivos; rode `pam composer install` depois.

Arquivos gerados:

```text
index.php                           sobe o servidor: carrega bootstrap/app.php e chama listen()
bootstrap/app.php                   monta e devolve o App, sem listen()
bin/nimbus                          console: bin/nimbus tinker
routes.php                          todas as rotas
app/Controllers/PingController.php
database/{factories,migrations,seeders}/
tests/TestCase.php
tests/ApplicationTest.php
phpunit.xml
composer.json                       pushinbr/nimbus e pushinbr/arcano; PHPUnit, Faker, Pint e PsySH em dev
.env.example                        PAM_PORT=3000
pam.json
```

`bootstrap/app.php` é o único lugar que monta a aplicação. O `index.php`, os testes (`tests/TestCase.php`) e o `bin/nimbus tinker` carregam o mesmo arquivo:

```php
// bootstrap/app.php
use Nimbus\App;

$root = dirname(__DIR__);

$app = new App();
$app->routes($root . '/routes.php');

return $app;
```

```php
// index.php
/** @var \Nimbus\App $app */
$app = require __DIR__ . '/bootstrap/app.php';

$app->listen((int) (getenv('PAM_PORT') ?: 3000));
```

```php
// routes.php
use App\Controllers\PingController;
use Nimbus\RouteRegistrar;

/** @var RouteRegistrar $router */

$router->get('api/ping', [PingController::class, 'show']);
```

```php
// app/Controllers/PingController.php
namespace App\Controllers;

use Pam\Http\Request;
use Pam\Http\Response;

final readonly class PingController
{
    public function show(Request $request, Response $response): Response
    {
        return $response->json(['message' => 'pong']);
    }
}
```

## Rodar

```bash
pam dev index.php
```

O `pam dev` recarrega os workers quando um arquivo PHP, o `.env` ou o `composer.json` muda. Em outro terminal:

```bash
curl -s http://127.0.0.1:3000/api/ping
```

```json
{"message":"pong"}
```

A porta vem de `PAM_PORT` (3000 quando a variável não existe).

## O primeiro teste

`tests/ApplicationTest.php` chama a mesma rota em memória, sem abrir porta:

```php
namespace Tests;

final class ApplicationTest extends TestCase
{
    public function test_ping_endpoint(): void
    {
        $this->getJson('/api/ping')
            ->assertOk()
            ->assertJsonPath('message', 'pong');
    }
}
```

```bash
pam test
```

O `pam test` roda o PHPUnit dentro do PAM. As requisições e asserções disponíveis estão em [Testes](./testes.md).

## A primeira entidade

O template `api` já traz o `pushinbr/arcano` no `composer.json`, mas não liga o banco. Registre o `ArcanoServiceProvider` no `bootstrap/app.php`:

```php
// bootstrap/app.php
use Nimbus\App;
use Nimbus\Database\ArcanoSchemaCompiler;
use Nimbus\Database\ArcanoServiceProvider;
use Nimbus\Database\DatabaseConfig;

$root = dirname(__DIR__);

$app = new App();
$app->provider(new ArcanoServiceProvider(
    DatabaseConfig::fromEnvironment(),
    new ArcanoSchemaCompiler($root . '/app/Models', $root . '/var/cache/arcano-schema.php', refresh: getenv('PAM_ENV') !== 'production'),
    migrations: $root . '/database/migrations',
));
$app->routes($root . '/routes.php');

return $app;
```

`DatabaseConfig::fromEnvironment()` lê as variáveis `DB_*` ([Banco de dados](./banco.md#conexao)). Para começar com SQLite, exporte no terminal:

```bash
export DB_CONNECTION=sqlite
export DB_DATABASE="$PWD/database/database.sqlite"
```

Adicione `/var/` ao `.gitignore`: é onde fica o cache do schema. A entidade vai em `app/Models`:

```php
// app/Models/Product.php
namespace App\Models;

use Arcano\Annotated\Annotation\Column;
use Arcano\Annotated\Annotation\Entity;

#[Entity(role: 'product', table: 'products')]
class Product
{
    #[Column(type: 'primary')]
    public ?int $id = null;

    #[Column(type: 'string(120)')]
    public string $name = '';

    #[Column(type: 'integer', name: 'price_in_cents')]
    public int $priceInCents = 0;
}
```

### Migration

O `ArcanoMigrationManager` gera a migration pela diferença entre as entidades e o banco. Pelo console do projeto ([Console](./console.md)):

```bash
pam exec bin/nimbus tinker --execute='$container->get(Nimbus\Database\ArcanoMigrationManager::class)->generate()'
pam exec bin/nimbus tinker --execute='$container->get(Nimbus\Database\ArcanoMigrationManager::class)->migrate()'
```

O primeiro comando grava um arquivo em `database/migrations/`; o segundo roda as migrations pendentes e as registra na tabela `migrations`. Revise o arquivo gerado antes de commitar; ele pode ser editado à mão.

### Rota e controller

```php
// app/Controllers/ProductController.php
namespace App\Controllers;

use App\Models\Product;
use Nimbus\Database\ArcanoManager;
use Pam\Http\Request;
use Pam\Http\Response;

final readonly class ProductController
{
    public function __construct(private ArcanoManager $arcano) {}

    public function index(Request $request, Response $response): Response
    {
        $products = $this->arcano->select(Product::class)->orderBy('id')->fetchAll();

        return $response->json(['data' => array_map(
            static fn (Product $product): array => [
                'id' => $product->id,
                'name' => $product->name,
                'price_in_cents' => $product->priceInCents,
            ],
            $products,
        )]);
    }
}
```

```php
// routes.php
$router->get('api/products', [App\Controllers\ProductController::class, 'index']);
```

O container injeta o `ArcanoManager` no construtor. Para respostas maiores, use [recursos JSON](./respostas.md) no lugar do `array_map`.

### Teste com banco

Aponte os testes para SQLite em memória no `phpunit.xml`, dentro de `<phpunit>`:

```xml
<php>
    <env name="DB_CONNECTION" value="sqlite"/>
    <env name="DB_DATABASE" value=":memory:"/>
</php>
```

```php
// tests/ProductTest.php
namespace Tests;

use App\Models\Product;
use Nimbus\Testing\RefreshDatabase;

final class ProductTest extends TestCase
{
    use RefreshDatabase;

    public function test_lists_products(): void
    {
        $product = new Product();
        $product->name = 'Teclado';
        $product->priceInCents = 25_000;
        $this->arcano()->entityManager()->persist($product)->run();

        $this->getJson('/api/products')
            ->assertOk()
            ->assertJsonCount(1, 'data')
            ->assertJsonPath('data.0.name', 'Teclado');
    }
}
```

`RefreshDatabase` roda as migrations pendentes e desfaz as escritas no fim de cada teste. Para gerar dados em volume, use [factories](./factories.md).

## Próximas páginas

- [Rotas e middlewares](./rotas.md) e [Controllers e container](./controllers.md)
- [Input e validação](./validacao.md)
- [Configuração tipada](./configuracao.md)
- [Deploy](./deploy.md)
