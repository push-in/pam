# Instalação e estrutura do app

## Criar um app

O `pam init` cria o projeto Nimbus com Composer, testes e `routes.php`:

```bash
pam init minha-api --template api   # API JSON (template padrão)
pam init meu-site --template web    # páginas renderizadas no servidor, com login
```

Num projeto PAM que já existe, instale o pacote pelo Composer embutido no PAM:

```bash
pam composer require pushinbr/nimbus
pam composer require pushinbr/arcano   # só se o app usar banco
```

### Template `api`

Gera um app mínimo com uma rota e um teste. O passo a passo até a primeira entidade está em [Primeiros passos](./primeiros-passos.md).

```text
app/Controllers/PingController.php
bootstrap/app.php
bin/nimbus
database/{factories,migrations,seeders}/
tests/TestCase.php
tests/ApplicationTest.php
routes.php
index.php
phpunit.xml
.env.example
```

```php
// bootstrap/app.php: monta o App sem listen(); index.php, testes e tinker carregam este arquivo
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

`bin/nimbus tinker` abre um shell PsySH sobre a mesma aplicação ([Console](./console.md)).

```php
// routes.php
use App\Controllers\PingController;
use Nimbus\RouteRegistrar;

/** @var RouteRegistrar $router */

$router->get('api/ping', [PingController::class, 'show']);
```

### Template `web`

Gera um app com sessão em arquivo (`storage/sessions`), CSRF em todo formulário, views `resources/views/**/*.nim.php` com o layout como componente `<x-layout>`, login, cadastro, logout, reset de senha por e-mail e um dashboard só para usuários logados. O usuário é uma entidade Arcano (`app/Models/User.php`) com senha em `Nimbus\Security\Hash`; os textos vêm de `lang/pt_BR` e `lang/en`.

`bootstrap/app.php` monta a aplicação para o `index.php`, o `bin/migrate`, o `bin/nimbus tinker` e os testes, e lê o `.env`, que o `pam init` cria com um `APP_KEY` aleatório. Não precisa de JavaScript.

```bash
cd meu-site
pam composer install
pam exec bin/migrate     # SQLite em storage/database.sqlite por padrão
pam test                 # testes de feature em SQLite na memória
pam dev index.php
```

`bin/migrate` aceita `migrate`, `rollback [passos]`, `status`, `cache` (gera o cache do schema do Arcano) e `seed`.

## Estrutura de um app

O mapa de pastas segue o do Laravel, com menos camadas. Todas as rotas ficam num único `routes.php`; não há rota declarada em model, controller ou atributo.

```text
app/
├── Models/          entidades Arcano
├── Controllers/
└── Jobs/
config/
database/{migrations,factories,seeders}/
tests/
routes.php           todas as rotas
.env
```

Pastas extras (`public/`, `resources/`, `lang/`, `app/Views`...) só existem quando o app usa. Os geradores criam cada arquivo no lugar certo e nunca sobrescrevem um arquivo existente:

```bash
pam make:model Product
pam make:controller ProductController
pam make:request StoreProductRequest
pam make:resource ProductResource
pam make:migration CreateProductsTable
pam make:job SendInvoice
pam make:factory ProductFactory
pam make:seeder DatabaseSeeder
pam make:test ProductApiTest
pam make:view products.index
```

`pam routes` lista as rotas do app.

## Rodar

```bash
pam dev index.php                              # desenvolvimento, com hot reload
pam start index.php --workers 4                # produção local
pam start index.php --workers 4 --task-workers 1   # com fila e agendador (veja Filas)
```

A porta vem de `PAM_PORT` (3000 no template). `App::listen($port, $host, $options)` aceita as opções do listener do PAM, como `maxBodyBytes` (2 MiB por padrão).

## Testar

```bash
pam test
```

O `pam test` roda o PHPUnit (ou Pest) dentro do PAM. Os testes estendem `Nimbus\Testing\TestCase` e mandam requisições para a aplicação em memória, sem abrir porta. Veja [Testes](./testes.md).

## Verificar o projeto

```bash
pam doctor          # dependências, autoload e configuração
pam views:check     # compila todas as views .nim.php (apps web)
```
