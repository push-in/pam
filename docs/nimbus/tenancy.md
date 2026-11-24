# Multi-tenant

`Nimbus\Tenancy` identifica o tenant (cliente, empresa, loja) de cada requisição e o deixa no escopo da requisição. Com o Arcano, `protectTenant()` limita as consultas e gravações das entidades ao tenant atual.

## Tenant e resolver

O tenant implementa `Nimbus\Tenancy\Tenant`, com um identificador estável:

```php
namespace App\Models;

use Arcano\Annotated\Annotation\Column;
use Arcano\Annotated\Annotation\Entity;
use Nimbus\Tenancy\Tenant;

#[Entity(role: 'company', table: 'companies')]
class Company implements Tenant
{
    #[Column(type: 'primary')]
    public ?int $id = null;

    #[Column(type: 'string(63)')]
    public string $subdomain = '';

    public function identifier(): string
    {
        return (string) $this->id;
    }
}
```

O resolver implementa `Nimbus\Tenancy\TenantResolver` e encontra o tenant na requisição: subdomínio, cabeçalho, claim do token. Devolve `null` quando não encontra:

```php
namespace App\Tenancy;

use App\Models\Company;
use Nimbus\Database\ArcanoManager;
use Nimbus\Tenancy\Tenant;
use Nimbus\Tenancy\TenantResolver;
use Pam\Http\Request;

final readonly class SubdomainResolver implements TenantResolver
{
    public function __construct(private ArcanoManager $arcano) {}

    public function resolve(Request $request): ?Tenant
    {
        $host = strtolower($request->getHeader('host') ?? '');
        $subdomain = strstr($host, '.', true);
        if ($subdomain === false || $subdomain === '') {
            return null;
        }
        return $this->arcano->select(Company::class)->where('subdomain', $subdomain)->fetchOne();
    }
}
```

Compare com uma lista fixa de domínios quando o app fica atrás de proxy, e não confie num cabeçalho `x-tenant` vindo do cliente sem checar se o usuário pertence ao tenant.

## Resolver por requisição

`Nimbus\Middleware\ResolveTenantMiddleware` chama o resolver e registra o tenant no escopo da requisição, como `Tenant` e como `TenantContext`:

```php
use App\Tenancy\SubdomainResolver;
use Nimbus\Middleware\ResolveTenantMiddleware;

$container = $app->container();
$app->middleware(new ResolveTenantMiddleware($container->get(SubdomainResolver::class), $container));
```

Sem tenant, responde `404` em problem+json (`Tenant was not found.`). Com `required: false`, a requisição segue sem tenant, o que serve para rotas públicas como cadastro e login:

```php
$public = $app->prefix('api/public')
    ->middleware(new ResolveTenantMiddleware($resolver, $container, required: false));
$public->post('signup', [SignupController::class, 'store']);
```

Um middleware global (`$app->middleware()`) roda em todas as rotas; para misturar rotas com e sem tenant obrigatório, registre o middleware por grupo, como acima.

Nos handlers, injete o tenant:

```php
use Nimbus\Tenancy\Tenant;

public function show(Request $request, Response $response, Tenant $tenant): Response
{
    return $response->json(['tenant' => $tenant->identifier()]);
}
```

O valor vive só no escopo da requisição: duas requisições concorrentes no mesmo worker, de tenants diferentes, nunca enxergam o tenant uma da outra, e nada sobra para a próxima requisição.

## Proteger entidades no Arcano

Marque, no boot, os papéis que pertencem a um tenant:

```php
use App\Models\Invoice;
use App\Models\Order;
use Nimbus\Database\ArcanoManager;

$arcano = $app->container()->get(ArcanoManager::class);
$arcano->protectTenant(Order::class);                // coluna tenant_id
$arcano->protectTenant(Invoice::class, 'company_id');
```

Para cada papel protegido:

- todo `select()` recebe `WHERE <coluna> = <identificador do tenant>`;
- uma entidade nova recebe a coluna do tenant atual ao ser gravada;
- leitura e escrita sem `TenantContext` ativo lançam `LogicException`, em vez de devolver dados de todos os tenants.

A coluna precisa existir na tabela; declare-a na entidade e na migration. Um papel com mapper próprio precisa estender `Nimbus\Database\ArcanoTenantMapper`. `protectTenant()` exige o `ArcanoManager` montado pelo `ArcanoServiceProvider` (com o container da aplicação).

Fora de uma requisição (jobs, comandos, tinker), não há `TenantContext`. Um job de um tenant leva o identificador no payload e abre o escopo antes de tocar entidades protegidas:

```php
use Nimbus\Tenancy\Tenant;
use Nimbus\Tenancy\TenantContext;

$company = $arcano->select(Company::class)->wherePK($this->companyId)->fetchOne();

$container->beginScope([Tenant::class => $company, TenantContext::class => new TenantContext($company)]);
try {
    // ... consultas limitadas ao tenant
} finally {
    $container->endScope();
}
```

`beginScope()` lança `LogicException` se já houver um escopo ativo na Fiber, como dentro de uma requisição.

O papel do próprio tenant (`Company` acima) não deve ser protegido: o resolver precisa consultá-lo antes de existir um tenant.

## Testes

Num teste de feature, mande o que o resolver lê, por exemplo o cabeçalho `host`:

```php
$this->withHeader('host', 'acme.example.com')
    ->getJson('/api/orders')
    ->assertOk();
```
