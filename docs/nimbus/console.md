# Console: `bin/nimbus`

`bin/nimbus tinker` abre um shell [PsySH](https://psysh.org) com a aplicação já inicializada: container, providers e Arcano prontos, sem subir o servidor. Serve para consultar o banco, chamar um serviço ou testar uma expressão.

## Comandos

Com `symfony/console` instalado (`pam composer require symfony/console`), o `bin/nimbus` carrega o `bootstrap/app.php` e roda:

| Comando | O que faz |
| --- | --- |
| `migrate [--force]` | roda as migrations pendentes do `ArcanoServiceProvider` (`migrations:`) |
| `migrate:status` | lista as migrations e quando rodaram |
| `migrate:rollback [--step=1]` | desfaz as últimas migrations |
| `db:seed [--class=Database\Seeders\DatabaseSeeder]` | roda um seeder e os que ele chama |
| `schema:cache` | recompila o cache do schema Arcano (`var/cache/arcano-schema.php`) que produção lê sem varrer as entidades |
| `schema:clear` | apaga esse cache |
| `schedule:list` | lista as tarefas do `Schedule` registrado no container |
| `tinker` | o shell PsySH descrito abaixo |
| `list`, `help <comando>` | os comandos e as opções |

`schema:cache` e `schema:clear` usam o `ArcanoSchemaCompiler` passado ao `ArcanoServiceProvider`; funcionam num app sem pasta de migrations, que lê um banco existente. `--force` é aceito para scripts de deploy do Laravel; o Nimbus não pergunta nada.

```bash
pam exec bin/nimbus schema:cache
pam exec bin/nimbus migrate --force
```

### Comandos da aplicação

Um comando é uma classe `Nimbus\Console\Command` (sobre o `symfony/console`) com a assinatura no formato do Laravel. Os parâmetros de `handle()` vêm do container.

```php
namespace App\Console\Commands;

use App\Services\AccountSync;
use Nimbus\Console\Command;

final class SyncAccounts extends Command
{
    protected $signature = 'buck:sync {account? : Uma conta} {--force} {--limit=100}';

    protected $description = 'Sincroniza as contas monitoradas.';

    public function handle(AccountSync $sync): int
    {
        $count = $sync->run($this->argument('account'), (bool) $this->option('force'), (int) $this->option('limit'));
        $this->info("{$count} contas sincronizadas.");
        return self::SUCCESS;
    }
}
```

- Classes em `app/Console/Commands` (namespace `App\Console\Commands`, autoload do Composer) são descobertas; outras entram com `$app->commands([SyncAccounts::class])`.
- A assinatura aceita `{arg}`, `{arg?}`, `{arg=padrão}`, `{arg*}`, `{--flag}`, `{--opcao=}`, `{--opcao=padrão}`, `{--O|opcao}`, `{--opcao=*}` e descrições depois de ` : `.
- Saída: `info()`, `comment()`, `warn()`, `error()`, `line()`, `newLine()`, `table()`; entrada: `ask()`, `confirm()`; `call('outro:comando', [...])` roda outro comando.
- Um comando escrito direto com `Symfony\Component\Console\Command\Command` também funciona.

### Agendar um comando

```php
$schedule = new Schedule($queue, timezone: 'America/Sao_Paulo', container: $app->container());
$schedule->command('buck:sync --force')->hourly()->withoutOverlapping();
$schedule->command(SyncAccounts::class, ['account' => '7'])->dailyAt('03:00');
```

O comando roda no task worker, no mesmo processo, pelo `Nimbus\Console\Kernel` do container. Um código de saída diferente de `0` falha a tarefa com a saída do comando na mensagem. A `Schedule` precisa do container.

## Tinker

`bin/nimbus tinker` funciona também sem `symfony/console`.

## O que o projeto precisa

Projetos criados com `pam init --template api` e o skeleton `pushinbr/nimbus-skeleton` já trazem as três peças:

- `bootstrap/app.php`, que monta e devolve o `App` sem chamar `listen()`. O `index.php` carrega esse arquivo e chama `listen()`; os testes e o tinker carregam o mesmo arquivo.
- `bin/nimbus`, que entra na raiz do projeto, carrega o `vendor/autoload.php` e repassa para o `vendor/pushinbr/nimbus/bin/nimbus`.
- `psy/psysh` em `require-dev`.

Num projeto mais antigo, crie os dois arquivos e instale o PsySH:

```bash
pam composer require --dev psy/psysh
```

```php
// bootstrap/app.php
use Nimbus\App;

$root = dirname(__DIR__);

$app = new App();
$app->routes($root . '/routes.php');
// providers e middlewares que hoje estão no index.php

return $app;
```

```php
// index.php
/** @var \Nimbus\App $app */
$app = require __DIR__ . '/bootstrap/app.php';

$app->listen((int) (getenv('PAM_PORT') ?: 3000));
```

```php
#!/usr/bin/env php
<?php
// bin/nimbus

declare(strict_types=1);

chdir(dirname(__DIR__));
$_composer_autoload_path = dirname(__DIR__) . '/vendor/autoload.php';
if (!is_file($_composer_autoload_path)) {
    fwrite(STDERR, "Composer dependencies are missing; run pam composer install.\n");
    exit(2);
}
require dirname(__DIR__) . '/vendor/pushinbr/nimbus/bin/nimbus';
```

O arquivo de bootstrap pode devolver o `App` ou uma closure que monta o `App`.

## Abrir o shell

```bash
pam exec bin/nimbus tinker
```

O `pam exec` roda o script com o PHP embutido no PAM, então funciona sem PHP instalado na máquina. Com um PHP 8.5 no `PATH`, `bin/nimbus tinker` também funciona direto.

O shell começa com estas variáveis:

| Variável | Valor |
| --- | --- |
| `$app` | o `Nimbus\App` inicializado (`boot()` já rodou) |
| `$container` | `$app->container()` |
| `$arcano` | o `Nimbus\Database\ArcanoManager`, quando o app registra o `ArcanoServiceProvider` |
| `$orm` | `$arcano->orm()` |

```text
Nimbus tinker: $app, $container, $arcano, $orm.
> $arcano->select(App\Models\Product::class)->count()
= 12
> $container->get(App\Services\ExchangeRates::class)->brlPerUsd()
= 5.42
```

As variáveis de ambiente valem como no servidor: o `DatabaseConfig::fromEnvironment()` lê o `DB_*` do shell que abriu o tinker. Cuidado com um terminal apontado para o banco de produção.

## Rodar um trecho sem abrir o shell

`--execute` roda o código e sai. Um resultado escalar é impresso com `var_export()`; outro valor, pelo tipo:

```bash
pam exec bin/nimbus tinker --execute='$arcano->select(App\Models\Product::class)->count()'
# 12

pam exec bin/nimbus tinker --execute='$container->get(Nimbus\Database\ArcanoMigrationManager::class)->migrate()'
# array
```

Use aspas simples no shell para que o `$` chegue ao PHP.

## Opções

```text
nimbus tinker [--bootstrap=bootstrap/app.php] [--execute=code]
```

- `--bootstrap=caminho` carrega outro arquivo. Sem a opção, o tinker procura `bootstrap/app.php` e depois `app.php` na pasta atual.
- `--execute=código` roda o código sem modo interativo.

Erros saem no stderr com código `1`; uma opção desconhecida sai com `2`. Sem arquivo de bootstrap, a mensagem é `No bootstrap file: create bootstrap/app.php returning the App, or pass --bootstrap=path.`; sem PsySH, `nimbus tinker requires psy/psysh: composer require --dev psy/psysh.`

No skeleton, `pam composer tinker` roda o mesmo comando pelo script do `composer.json`.
