# Documentação do Nimbus

Nimbus é o framework PHP para APIs que roda sobre o runtime PAM. É o pacote Composer `pushinbr/nimbus` (versão 0.6.0), substitui o `pushinbr/pam-http` (congelado na 2.2.x) e usa o Arcano como único ORM.

A aplicação roda em workers persistentes do PAM: o código de boot (rotas, container, schema do ORM, templates) roda uma vez por worker, e cada requisição roda numa Fiber própria, com conexões de banco, identity map e valores `scoped` isolados.

Requisitos: PAM instalado (`curl -fsSL https://push-in.github.io/pam/install.sh | sh`) e PHP 8.5, que o próprio PAM embute.

## Guia

**Começo**

- [Primeiros passos](./primeiros-passos.md): `pam init --template api`, `/api/ping`, o primeiro teste, a primeira entidade e migration.
- [Instalação e estrutura do app](./instalacao.md): templates, pastas, `routes.php`, rodar e testar.
- [Configuração tipada](./configuracao.md): `Configuration`, `ConfigDefinition` e variáveis de ambiente.
- [Console: `bin/nimbus tinker`](./console.md)

**HTTP**

- [Rotas e middlewares](./rotas.md)
- [Controllers e container](./controllers.md)
- [Input e validação](./validacao.md)
- [Recursos JSON e paginação](./respostas.md)
- [Erros: problem+json e `ProblemCode`](./erros.md)
- [OpenAPI e clientes gerados](./openapi.md)
- [Cliente HTTP](./http-client.md): `Http::withToken()->retry()->post()`, pool, fakes.

**Dados**

- [Banco de dados com Arcano](./banco.md): entidades, migrations, pool, Postgres não bloqueante.
- [Factories e seeders](./factories.md)
- [Cache da aplicação e tags](./cache.md)
- [Redis, cache de resposta, rate limit e idempotência](./redis.md)
- [Multi-tenant](./tenancy.md)
- [Storage e URLs assinadas](./storage.md)

**Segurança**

- [Autenticação](./autenticacao.md): bearer HMAC, tokens pessoais, refresh, JWT, OAuth, reset de senha, verificação de e-mail.
- [Autorização com Gate e policies](./autorizacao.md)
- [Hash, Crypt, CORS e cabeçalhos](./seguranca.md)

**Segundo plano**

- [Filas e agendador](./filas.md)
- [Eventos e broadcasting](./eventos.md): listeners, listeners na fila, canais WebSocket.
- [Notificações](./notificacoes.md): e-mail, push, banco e webhook.
- [E-mail com `Mailer`](./email.md)

**Web (opcional)**

- [Sessão, cookies e CSRF](./sessao.md)
- [Views `.nim.php`](./views.md)
- [Tradução](./traducao.md)
- [Painel administrativo sobre o Arcano](./admin.md)

**Operação**

- [Testes](./testes.md)
- [Logs e observabilidade](./observabilidade.md): observadores, profiler, painel de debug, health checks.
- [Retentativas, circuit breaker, transações e prazos](./resiliencia.md)
- [Deploy com `pam deploy:remote`](./deploy.md)
- [Desempenho: números do benchmark](./desempenho.md)
- [Migração do Laravel](./migracao-laravel.md)

## Referências

- [API pública completa](https://github.com/push-in/pam/blob/main/packages/nimbus/docs/PUBLIC-API.md), gerada a partir do `api-surface.json`.
- [Changelog](https://github.com/push-in/pam/blob/main/packages/nimbus/CHANGELOG.md) e [guia de atualização](https://github.com/push-in/pam/blob/main/packages/nimbus/UPGRADE.md).
- [Política de versões](https://github.com/push-in/pam/blob/main/packages/nimbus/docs/VERSIONING.md): o que é API pública, ciclo de deprecação de uma minor, suporte da 1.x (correções de segurança por 18 meses depois da 2.0).
- [Regras Rector Laravel → Nimbus](https://github.com/push-in/pam/tree/main/packages/nimbus-rector) e [plano do piloto com um serviço da Pushin](https://github.com/push-in/pam/blob/main/docs/dogfood-plan.md).
- [Referência do CLI `pam`](https://github.com/push-in/pam/blob/main/docs/cli.md).

Até a 1.0 o Nimbus segue SemVer na linha 0.x: uma versão minor ainda pode ajustar a API pública, sempre registrada no changelog. A partir da 1.0 vale a [política de versões](https://github.com/push-in/pam/blob/main/packages/nimbus/docs/VERSIONING.md), conferida no CI pelo `api-surface.json` e pelo `roave/backward-compatibility-check` contra a última tag.
