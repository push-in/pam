# Console: `bin/nimbus tinker`

`bin/nimbus tinker` abre um shell [PsySH](https://psysh.org) com a aplicação já inicializada: container, providers e Arcano prontos, sem subir o servidor. Serve para consultar o banco, chamar um serviço ou testar uma expressão.

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
