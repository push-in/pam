# Autenticação

A autenticação gira em torno de um contrato: `Nimbus\Auth\Authenticator`, que recebe a requisição e devolve um `Principal` (`identifier()` e `can($ability)`) ou `null`. O app registra uma implementação no container, e `auth()` nas rotas passa a exigir um principal:

```php
use Nimbus\Auth\Authenticator;

$app->container()->instance(Authenticator::class, $authenticator);

$router->get('me', [MeController::class, 'show'])->auth();
$router->auth()->prefix('account')->group(function (RouteRegistrar $router): void { /* ... */ });
```

Sem principal, a resposta é `401` (`Unauthenticated`). Com principal, `AuthenticateMiddleware` coloca no escopo da requisição o `Principal` e o `AuthContext`, que a action pode receber:

```php
use Nimbus\Auth\Principal;

public function show(Principal $user, Response $response): Response
{
    return $response->json(['id' => $user->identifier()]);
}
```

Implementações incluídas:

| Classe | Credencial |
| --- | --- |
| `BearerTokenAuthenticator` | token assinado (`HmacTokenCodec` ou outro `AccessTokenVerifier`) |
| `PersonalAccessTokenAuthenticator` | token pessoal gravado no banco |
| `JwtAuthenticator` | JWT (`JwtCodec`) |
| `SessionAuthenticator` | sessão web ([Sessão](./sessao.md)) |
| `ChainAuthenticator` | tenta várias em ordem |

## Tabelas

Tokens pessoais, refresh, tokens de uso único, revogações e estado OAuth usam as tabelas `personal_access_tokens`, `refresh_tokens`, `auth_tokens`, `revoked_tokens` e `oauth_states`. Publique a migration uma vez e rode as migrations:

```php
$migrations->publish('create_auth_tables', \Nimbus\Auth\AuthTablesMigration::class);
```

Todo token gravado é o SHA-256 de um segredo aleatório de 256 bits, comparado em tempo constante. O token em texto aparece só na resposta que o entrega.

Nos exemplos abaixo, `$databases` é o `ArcanoDatabaseProvider` registrado pelo provider do Arcano:

```php
use Nimbus\Database\ArcanoDatabaseProvider;

$databases = $app->container()->get(ArcanoDatabaseProvider::class);
```

## Token bearer assinado (HMAC)

`HmacTokenCodec` emite e valida tokens de acesso HS256 sem consultar o banco: tamanho limitado, assinatura, emissor e audiência comparados em tempo constante, `iat`/`nbf`/`exp`, ID único e abilities.

```php
use Nimbus\Auth\BearerTokenAuthenticator;
use Nimbus\Auth\HmacTokenCodec;

$tokens = new HmacTokenCodec(
    secret: $secretDoGerenciadorDeSegredos,   // mínimo de 32 bytes, fora do controle de versão
    issuer: 'https://auth.example.com',
    audience: 'orders-api',
);

$app->container()->instance(Authenticator::class, new BearerTokenAuthenticator($tokens));

$token = $tokens->issue('user:42', ['orders:read'], ttlSeconds: 900);
```

O token de acesso dura no máximo 24 horas; renove com refresh tokens em vez de emitir tokens longos. Para trocar a chave sem derrubar sessões, defina `keyIdentifier` na chave ativa e passe até quatro chaves antigas em `verificationKeys`. Um `kid` desconhecido é recusado antes de qualquer claim ser lido.

## Tokens pessoais com abilities

Tokens para CLI, integrações e apps, guardados no banco:

```php
use Nimbus\Auth\PersonalAccessTokenAuthenticator;
use Nimbus\Auth\PersonalAccessTokens;

$tokens = new PersonalAccessTokens($databases);
$app->container()->instance(Authenticator::class, new PersonalAccessTokenAuthenticator($tokens));

// numa action autenticada:
$new = $tokens->create('user:42', 'CLI', ['products:read', 'products:write']);
return $response->json($new->toArray(), 201);   // mostra o token em texto uma única vez
```

```php
$router->post('products', [ProductController::class, 'store'])->can('products:write');
```

- Formato: `nbp_{id}.{segredo}`.
- `create($subject, $name, $abilities, $expiresAt)`: `expiresAt` opcional (timestamp Unix).
- `last_used_at` é atualizado no máximo uma vez por minuto.
- `tokensFor($subject)` lista; `revoke($id)` e `revokeAll($subject)` revogam.
- Abilities aceitam `*` e curingas por namespace (`products:*`, `orders.*`).

## Access token e refresh token

`TokenPairIssuer` emite o par no formato de resposta OAuth 2 (`token_type`, `access_token`, `expires_in`, `refresh_token`, `scope`). O access token vem de um `HmacTokenCodec` ou `JwtCodec`; o refresh token fica no banco.

```php
use Nimbus\Auth\RefreshTokens;
use Nimbus\Auth\TokenPairIssuer;
use Nimbus\Security\Hash;

$pairs = new TokenPairIssuer($codec, new RefreshTokens($databases), accessTtlSeconds: 900);

// login: depois de conferir a senha
if ($user === null || !Hash::check($input->password, $user->password)) {
    throw new HttpException(401, ProblemCode::Unauthenticated, 'Invalid credentials.');
}
return $response->json($pairs->issue('user:' . $user->id, ['orders:read'])->toArray());
```

- `refresh($refreshToken)` gasta o refresh token e devolve um par novo da mesma família.
- Um refresh token já gasto que aparece de novo revoga a família inteira e responde `401` (detecção de reuso).
- `RefreshTokens::revokeAll($subject)` desloga o usuário de todos os dispositivos.
- Para revogar um access token sem estado antes de expirar, grave o `jti` (`TokenPrincipal::$tokenIdentifier`) num `TokenRevocationStore` compartilhado: `DatabaseTokenRevocationStore` ou `Nimbus\Redis\RedisTokenRevocationStore`, passado ao `BearerTokenAuthenticator` ou ao `JwtAuthenticator`. O `MemoryTokenRevocationStore` vale só para um worker.

`ChainAuthenticator` combina formas de autenticação, por exemplo access tokens primeiro e tokens pessoais depois:

```php
use Nimbus\Auth\ChainAuthenticator;

$app->container()->instance(Authenticator::class, new ChainAuthenticator(
    new BearerTokenAuthenticator($codec, $revocations),
    new PersonalAccessTokenAuthenticator(new PersonalAccessTokens($databases)),
));
```

## JWT

`JwtCodec` usa o `firebase/php-jwt`:

```bash
pam composer require firebase/php-jwt
```

```php
use Nimbus\Auth\JwtAuthenticator;
use Nimbus\Auth\JwtCodec;

$jwt = JwtCodec::hmac($secret, 'https://api.example.com', 'orders-api');                 // HS256/384/512
$jwt = JwtCodec::asymmetric($publicPem, $privatePem, $issuer, $audience, 'ES256');       // RS256, ES256, EdDSA
$jwt = JwtCodec::fromJwks($jwksDocument, 'https://idp.example.com', 'orders-api');       // só verifica

$app->container()->instance(Authenticator::class, new JwtAuthenticator($jwt, $revocations));
```

Cada chave fica presa ao seu algoritmo, e o token precisa ter `kid`, `iss`, `aud`, `sub` e `exp`. As abilities vêm na claim `scope` (os arrays `scp` e `abilities` também são aceitos). `JwtCodec::fromKeySet(new CachedKeySet(...))` verifica contra um JWKS remoto com cache via PSR-18/PSR-6.

## Login social (OAuth)

`Nimbus\Auth\OAuth\SocialLogin` executa o fluxo authorization code de qualquer provider do `league/oauth2-client` para clientes de API. O estado e o verificador PKCE ficam no servidor.

```php
use League\OAuth2\Client\Provider\Github;
use League\OAuth2\Client\Provider\Google;
use Nimbus\Auth\OAuth\DatabaseOAuthStateStore;
use Nimbus\Auth\OAuth\SocialLogin;

$social = new SocialLogin(
    ['google' => new Google([...]), 'github' => fn () => new Github([...])],
    new DatabaseOAuthStateStore($databases),     // ou Nimbus\Redis\RedisOAuthStateStore
    $app->container()->get(SocialUsers::class),  // seu SocialUserResolver: encontra ou cria o usuário
    $pairs,                                      // qualquer LoginTokenIssuer
    abilities: ['*'],
);
```

- `GET /auth/oauth/{provider}` devolve `authorization_url` e `state`.
- `POST /auth/oauth/{provider}/callback` com `code` e `state` troca o código e devolve um token do Nimbus.

As rotas são registradas pelo `AuthRoutes`, abaixo.

## Reset de senha e verificação de e-mail

Implemente `Nimbus\Auth\AuthUser` na entidade de usuário (`authIdentifier()`, `hasVerifiedEmail()` e as rotas de notificação, com o trait `RoutesNotifications`) e `Nimbus\Auth\AuthUserProvider` no repositório (`findById`, `findByEmail`, `updatePassword`, `markEmailAsVerified`). Depois registre as rotas:

```php
use Nimbus\Auth\AuthRoutes;
use Nimbus\Auth\EmailVerification;
use Nimbus\Auth\OneTimeTokens;
use Nimbus\Auth\PasswordResets;
use Nimbus\Redis\RedisRateLimitStore;

$oneTime = new OneTimeTokens($databases);

(new AuthRoutes(
    passwords: new PasswordResets($users, $oneTime, $notifier, 'https://app.example.com/reset?token={token}',
        afterReset: fn (AuthUser $user) => $refreshTokens->revokeAll($user->authIdentifier())),
    verification: new EmailVerification($users, $oneTime, $notifier, 'https://app.example.com/verify?token={token}'),
    tokens: $pairs,
    social: $social,
    throttleStore: new RedisRateLimitStore($redis),
))->register($app, '/auth');
```

| Rota | Corpo | Resposta |
| --- | --- | --- |
| `POST /auth/password/forgot` | `email` | 202, igual para e-mail desconhecido |
| `POST /auth/password/reset` | `token`, `password` | 204 ou 422 |
| `POST /auth/email/verify` | `token` | 204 ou 422 |
| `POST /auth/email/resend` | autenticado | 202 |
| `POST /auth/token/refresh` | `refresh_token` | par de tokens ou 401 |
| `POST /auth/token/revoke` | `refresh_token` | 204 |

- Tokens de reset e verificação são de uso único, expiram (uma hora e um dia por padrão) e substituem o token anterior do usuário.
- O link sai pelas [Notificações](./notificacoes.md) com `send()`, nunca pela fila, para o token não entrar no payload de jobs. Passe `notification:` para personalizar o `MailMessage`.
- Cada usuário recebe no máximo um e-mail por minuto, e toda rota tem limite por IP.
- A senha nova é gravada com `Nimbus\Security\Hash`.

## Login web por sessão

Para apps com páginas, `SessionAuthenticator::login($user)` grava o usuário na sessão e regenera o ID da sessão; `logout()` invalida a sessão. O template `web` do `pam init` já traz login, cadastro, logout e reset de senha prontos. Veja [Sessão](./sessao.md).
