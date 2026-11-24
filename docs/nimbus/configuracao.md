# Configuração tipada

`Nimbus\Config\Configuration` lê variáveis de ambiente a partir de uma lista de definições, converte cada valor para o tipo declarado e falha no boot com todos os erros de uma vez quando falta uma variável obrigatória ou um valor não converte.

## Declarar

```php
use Nimbus\Config\ConfigDefinition;
use Nimbus\Config\ConfigType;
use Nimbus\Config\Configuration;

$config = Configuration::fromEnvironment([
    new ConfigDefinition('app.port', 'PAM_PORT', ConfigType::Integer, required: false, default: 3000),
    new ConfigDefinition('app.debug', 'APP_DEBUG', ConfigType::Boolean, required: false, default: false),
    new ConfigDefinition('auth.secret', 'AUTH_SECRET', sensitive: true),
    new ConfigDefinition('payments.timeout', 'PAYMENTS_TIMEOUT', ConfigType::Float, required: false, default: 5.0),
]);

$app->container()->instance(Configuration::class, $config);
```

`ConfigDefinition($key, $environment, $type, $required, $default, $sensitive)`:

| Parâmetro | Padrão | Regra |
| --- | --- | --- |
| `key` | | minúsculas separadas por ponto: `app.port`, `mail.from_address` |
| `environment` | | nome da variável em maiúsculas: `PAM_PORT` |
| `type` | `ConfigType::String` | `String`, `Integer`, `Boolean` ou `Float` |
| `required` | `true` | uma definição obrigatória não aceita `default` |
| `default` | `null` | precisa ser do tipo declarado (`5.0` para `Float`, não `5`) |
| `sensitive` | `false` | mascarado em `redacted()` |

Uma variável vazia conta como ausente: com `required: true` é erro, senão vale o `default`. Booleanos aceitam `true`, `false`, `1`, `0`, `yes`, `no`, `on` e `off`.

## Ler

```php
$port = $config->integer('app.port');
$debug = $config->boolean('app.debug');
$secret = $config->string('auth.secret');
$timeout = $config->float('payments.timeout');
```

Cada leitor confere o tipo: `integer()` numa chave `String` lança `LogicException`, e uma chave não declarada lança `OutOfBoundsException`. Uma chave opcional sem `default` e sem variável vale `null`, e então nenhum dos leitores a aceita; declare um `default` quando o código precisar sempre de um valor.

## Erros de boot

`fromEnvironment()` junta todos os problemas numa única `Nimbus\Config\ConfigurationException`:

```text
Application configuration is invalid: AUTH_SECRET is required; PAM_PORT must be Integer
```

A lista fica também em `$exception->errors`. As mensagens citam a variável e o tipo esperado, nunca o valor recebido, então podem ir para o log.

## Diagnóstico

`redacted()` devolve todas as chaves em ordem alfabética, com `[REDACTED]` no lugar dos valores sensíveis:

```php
$log->info('config', $config->redacted());
// ['app.debug' => false, 'app.port' => 3000, 'auth.secret' => '[REDACTED]', 'payments.timeout' => 5.0]
```

## Testes

`fromArray()` recebe as mesmas definições e um array no lugar do ambiente do processo:

```php
$config = Configuration::fromArray($definitions, [
    'AUTH_SECRET' => 'test-secret',
    'PAM_PORT' => '8080',
]);
```

## Variáveis lidas pelos módulos

Os módulos do Nimbus leem as próprias variáveis com `fromEnvironment()`:

| Classe | Variáveis | Página |
| --- | --- | --- |
| `DatabaseConfig` | `DB_CONNECTION`, `DB_HOST`, `DB_PORT`, `DB_DATABASE`, `DB_USERNAME`, `DB_PASSWORD`, `DB_ASYNC` | [Banco](./banco.md) |
| `RedisPool` | `REDIS_HOST`, `REDIS_PORT`, `REDIS_PASSWORD`, `REDIS_DB` | [Redis](./redis.md) |
| `Mailer`, `MailChannel` | `MAILER_DSN` ou `MAIL_MAILER`, `MAIL_HOST`, `MAIL_PORT`, `MAIL_USERNAME`, `MAIL_PASSWORD`, `MAIL_FROM_ADDRESS`, `MAIL_FROM_NAME` | [E-mail](./email.md) |
| `PusherBroadcaster` | `PUSHER_APP_ID`, `PUSHER_APP_KEY`, `PUSHER_APP_SECRET`, `PUSHER_HOST`, `PUSHER_PORT`, `PUSHER_SCHEME`, `PUSHER_APP_CLUSTER` | [Eventos e broadcasting](./eventos.md) |
| `DebugPanel` | `APP_ENV` ou `PAM_ENV` | [Observabilidade](./observabilidade.md#painel-de-debug) |

O template `api` não lê `.env`: exporte as variáveis no shell ou no gerenciador de processos. O template `web` lê o `.env` no `bootstrap/app.php`.
