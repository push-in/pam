# Banco de dados com Arcano

O ORM do Nimbus é o Arcano (`pushinbr/arcano`), um data mapper derivado do Cycle ORM: entidades com atributos, relações, identity map, migrations e drivers para PostgreSQL, MySQL, SQLite e SQL Server. O núcleo do Nimbus não depende de banco; instale o Arcano quando o app precisar:

```bash
pam composer require pushinbr/arcano
```

## Conexão

`DatabaseConfig::fromEnvironment()` lê as variáveis:

| Variável | Padrão |
| --- | --- |
| `DB_CONNECTION` | `pgsql` (também `mysql`, `sqlite`, `sqlsrv`) |
| `DB_HOST`, `DB_PORT` | `127.0.0.1`, porta padrão do driver |
| `DB_DATABASE` | `pam` (`:memory:` no SQLite) |
| `DB_USERNAME`, `DB_PASSWORD` | `pam`, vazio |
| `DB_SSLMODE`, `DB_SOCKET`, `DB_TRUST_SERVER_CERTIFICATE` | opcionais |
| `DB_ASYNC` | `true` liga o driver Postgres não bloqueante |

PostgreSQL é o padrão de produção; SQLite serve para testes e apps pequenos.

## Registrar o provider

```php
use Nimbus\Database\ArcanoSchemaCompiler;
use Nimbus\Database\ArcanoServiceProvider;
use Nimbus\Database\DatabaseConfig;

$production = getenv('PAM_ENV') === 'production';

$app->provider(new ArcanoServiceProvider(
    DatabaseConfig::fromEnvironment(),
    new ArcanoSchemaCompiler(__DIR__ . '/app/Models', __DIR__ . '/storage/cache/arcano-schema.php', refresh: !$production),
    migrations: __DIR__ . '/database/migrations',
));
```

O compilador lê as entidades do diretório e grava o schema num arquivo PHP de cache. Em desenvolvimento o cache é refeito quando um arquivo de entidade muda; em produção use `refresh: false` e gere o cache no deploy (`bin/migrate cache` no template web). No lugar do compilador, o provider aceita um array de schema já compilado ou um `Arcano\ORM\Schema`.

O provider registra no container `ArcanoManager`, `ArcanoMigrationManager`, `ArcanoDatabaseProvider`, `QueryMonitor` e o `TransactionManager`.

## Entidades

Entidades são classes comuns com atributos, em `app/Models`:

```php
namespace App\Models;

use Arcano\Annotated\Annotation\Column;
use Arcano\Annotated\Annotation\Entity;
use Arcano\Annotated\Annotation\Relation\BelongsTo;
use Arcano\Annotated\Annotation\Relation\HasMany;

#[Entity(role: 'author', table: 'authors')]
class Author
{
    #[Column(type: 'primary')]
    public ?int $id = null;

    #[Column(type: 'string(120)')]
    public string $name = '';

    /** @var list<Post> */
    #[HasMany(target: Post::class, collection: 'array')]
    public array $posts = [];
}

#[Entity(role: 'post', table: 'posts')]
class Post
{
    #[Column(type: 'primary')]
    public ?int $id = null;

    #[Column(type: 'string(200)')]
    public string $title = '';

    #[Column(type: 'datetime', name: 'published_at', nullable: true)]
    public ?\DateTimeImmutable $publishedAt = null;

    #[BelongsTo(target: Author::class)]
    public Author $author;
}
```

## Consultar e gravar

Resolva `Nimbus\Database\ArcanoManager` (no construtor do controller ou pelo container):

```php
use Nimbus\Database\ArcanoManager;

$author = $arcano->select(Author::class)->load('posts')->wherePK($id)->fetchOne();

$recent = $arcano->select(Post::class)
    ->where('publishedAt', '>', new \DateTimeImmutable('-7 days'))
    ->orderBy('publishedAt', 'DESC')
    ->limit(20)
    ->fetchAll();

$author->name = 'Ada';
$arcano->entityManager()->persist($author)->run();

$arcano->entityManager()->delete($post)->run();
```

`repository(Post::class)` devolve o repositório do papel; `database()` dá acesso ao query builder sem entidades.

### Transações

```php
$arcano->transaction(function () use ($arcano, $order): void {
    $arcano->entityManager()->persist($order)->run();
    // ...
});
```

Para envolver uma requisição inteira, use `TransactionalMiddleware` na rota ou no grupo.

## Isolamento por requisição

Cada Fiber de requisição tem suas próprias conexões e seu identity map, então transações e entidades carregadas não vazam entre requisições do mesmo worker. No fim da requisição, transações abertas sofrem rollback e as conexões voltam para o pool.

## Pool de conexões

Cada worker mantém as conexões abertas entre requisições: a conexão liberada volta para um pool do worker e a próxima requisição reaproveita a sessão, sem pagar conexão e autenticação de novo. `maxIdleConnections` (8 por padrão) limita quantas ficam abertas por worker:

```php
new ArcanoServiceProvider($config, $schema, migrations: $dir, maxIdleConnections: 16);
```

Dimensione `workers × maxIdleConnections` abaixo do `max_connections` do Postgres.

## Postgres não bloqueante

Com `DB_ASYNC=true` e a extensão `pgsql`, o `Nimbus\Database\Pgsql\AsyncPostgresDriver` substitui o PDO no Postgres: dentro do PAM, uma query suspende só a Fiber da requisição, e o worker atende outras requisições enquanto espera o banco. Um comando interrompido fecha a sessão em vez de devolvê-la ao pool.

O PDO bloqueante continua sendo o padrão porque é mais rápido para queries curtas. O driver assíncrono compensa quando há queries longas ou latência de rede alta até o banco.

## Migrations

`ArcanoMigrationManager` gera migrations pela diferença entre as entidades e o banco, executa e reverte. Cada migration roda na própria transação e fica registrada na tabela `migrations`.

```php
use Nimbus\Database\ArcanoMigrationManager;

$migrations = $app->container()->get(ArcanoMigrationManager::class);

$migrations->generate();   // novos arquivos para as entidades alteradas
$migrations->migrate();    // roda as pendentes e devolve os nomes
$migrations->rollback(1);  // reverte a última
$migrations->status();     // name, created_at e executed_at de cada uma
```

A geração exige que todas as migrations existentes já tenham rodado. Os arquivos gerados podem ser editados, e migrations podem ser escritas à mão com a API de tabelas do Arcano:

```php
namespace Migration;

use Arcano\Migrations\Migration;

class CreateProducts extends Migration
{
    protected const DATABASE = 'default';

    public function up(): void
    {
        $this->table('products')
            ->addColumn('id', 'primary', ['nullable' => false])
            ->addColumn('name', 'string', ['nullable' => false, 'size' => 120])
            ->addColumn('price_in_cents', 'integer', ['nullable' => false])
            ->addIndex(['name'])
            ->setPrimaryKeys(['id'])
            ->create();
    }

    public function down(): void
    {
        $this->table('products')->drop();
    }
}
```

Módulos do Nimbus que precisam de tabelas (tokens, filas, sessões) trazem a migration pronta; `publish()` copia para o diretório do app:

```php
$migrations->publish('create_auth_tables', \Nimbus\Auth\AuthTablesMigration::class);
$migrations->publish('create_queue_tables', \Nimbus\Jobs\QueueTablesMigration::class);
$migrations->publish('create_session_tables', \Nimbus\Session\SessionTablesMigration::class);
```

O template web traz `bin/migrate` (`migrate`, `rollback [passos]`, `status`, `cache`, `seed`), que o `pam deploy:remote` roda a cada release.

## Orçamento de queries

`QueryBudgetMiddleware` limita a quantidade de queries, o tempo acumulado no banco e queries repetidas (N+1) por requisição. Use em desenvolvimento e em rotas sensíveis a desempenho:

```php
use Nimbus\Database\QueryBudget;
use Nimbus\Database\QueryMonitor;
use Nimbus\Middleware\QueryBudgetMiddleware;

$app->middleware(new QueryBudgetMiddleware(
    monitor: $app->container()->get(QueryMonitor::class),
    budget: new QueryBudget(
        maximumQueries: 30,
        maximumElapsedMilliseconds: 100,
        maximumDuplicateQueries: 3,
    ),
    failOnViolation: true,
));
```

As violações usam o enum inteiro `QueryBudgetViolation`, estável para relatórios de CI.

## Multi-tenant

Proteja os papéis que pertencem a um tenant uma vez no boot:

```php
$arcano->protectTenant(Order::class);              // coluna tenant_id
$arcano->protectTenant(Invoice::class, 'company_id');
```

Os selects ficam limitados ao tenant atual, entidades novas recebem a chave dele, e leitura e escrita falham sem um `TenantContext` ativo (resolvido pelo `ResolveTenantMiddleware`).

## Health check

```php
use Nimbus\Database\DatabaseHealthCheck;
use Nimbus\Health\HealthRegistry;

$health = (new HealthRegistry())->add('database', $app->container()->get(DatabaseHealthCheck::class));
```

O resultado traz latência e classe da falha, sem expor credenciais.
