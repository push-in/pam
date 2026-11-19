# Documentação do Nimbus

Nimbus é o framework PHP para APIs que roda sobre o runtime PAM. É o pacote Composer `pushinbr/nimbus` (versão 0.4.0), substitui o `pushinbr/pam-http` (congelado na 2.2.x) e usa o Arcano como único ORM.

A aplicação roda em workers persistentes do PAM: o código de boot (rotas, container, schema do ORM, templates) roda uma vez por worker, e cada requisição roda numa Fiber própria, com conexões de banco, identity map e valores `scoped` isolados.

Requisitos: PAM instalado (`curl -fsSL https://push-in.github.io/pam/install.sh | sh`) e PHP 8.5, que o próprio PAM embute.

## Guia

**Começo**

- [Instalação e estrutura do app](./instalacao.md): `pam init`, pastas, `routes.php`, rodar e testar.

**HTTP**

- [Rotas e middlewares](./rotas.md)
- [Controllers e container](./controllers.md)
- [Input e validação](./validacao.md)
- [Recursos JSON e paginação](./respostas.md)
- [Erros: problem+json e `ProblemCode`](./erros.md)
- [OpenAPI e clientes gerados](./openapi.md)

**Dados**

- [Banco de dados com Arcano](./banco.md): entidades, migrations, pool, Postgres não bloqueante.
- [Factories e seeders](./factories.md)
- [Redis, cache de resposta, rate limit e idempotência](./redis.md)
- [Storage e URLs assinadas](./storage.md)

**Segurança**

- [Autenticação](./autenticacao.md): bearer HMAC, tokens pessoais, refresh, JWT, OAuth, reset de senha, verificação de e-mail.
- [Autorização com Gate e policies](./autorizacao.md)
- [Hash, Crypt, CORS e cabeçalhos](./seguranca.md)

**Segundo plano**

- [Filas e agendador](./filas.md)
- [Notificações](./notificacoes.md): e-mail, push, banco e webhook.

**Web (opcional)**

- [Sessão, cookies e CSRF](./sessao.md)
- [Views `.nim.php`](./views.md)
- [Tradução](./traducao.md)
- [Painel administrativo sobre o Arcano](./admin.md)

**Operação**

- [Testes](./testes.md)
- [Logs e observabilidade](./observabilidade.md)
- [Deploy com `pam deploy:remote`](./deploy.md)
- [Desempenho: números do benchmark](./desempenho.md)
- [Migração do Laravel](./migracao-laravel.md)

## Referências

- [API pública completa](https://github.com/push-in/pam/blob/main/packages/nimbus/docs/PUBLIC-API.md), gerada a partir do `api-surface.json`.
- [Changelog](https://github.com/push-in/pam/blob/main/packages/nimbus/CHANGELOG.md) e [guia de atualização](https://github.com/push-in/pam/blob/main/packages/nimbus/UPGRADE.md).
- [Referência do CLI `pam`](https://github.com/push-in/pam/blob/main/docs/cli.md).

Até a 1.0 o Nimbus segue SemVer na linha 0.x: uma versão minor ainda pode ajustar a API pública, sempre registrada no changelog.
