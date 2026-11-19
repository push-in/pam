# Hash, Crypt, CORS e cabeçalhos

## Hash de senha

`Nimbus\Security\Hash` usa Argon2id (bcrypt quando o PHP não tem Argon2):

```php
use Nimbus\Security\Hash;

$user->password = Hash::make($input->password);

if (!Hash::check($input->password, $user->password)) {
    throw new HttpException(401, ProblemCode::Unauthenticated, 'Invalid credentials.');
}

if (Hash::needsRehash($user->password)) {
    $user->password = Hash::make($input->password);
}
```

## Criptografia

`Nimbus\Security\Crypt` cifra com sodium secretbox (cifra autenticada) usando o `APP_KEY`:

```php
use Nimbus\Security\Crypt;

$crypt = Crypt::fromEnvironment();          // APP_KEY=base64:<32 bytes aleatórios>
$payload = $crypt->encrypt('12345678900');
$cpf = $crypt->decrypt($payload);           // DecryptException se o payload foi alterado
```

`Crypt::generateKey()` gera uma chave nova no formato `base64:...`. O `pam init` e o `pam deploy:remote --init` já criam o `APP_KEY`. A mesma chave deriva a chave do `UrlSigner` ([URLs assinadas](./storage.md#urls-assinadas)) e dos cookies cifrados ([Sessão](./sessao.md)).

## CORS

```php
use Nimbus\Middleware\CorsMiddleware;

$app->middleware(new CorsMiddleware(
    origins: ['https://app.example.com'],
    methods: ['GET', 'POST', 'PUT', 'PATCH', 'DELETE', 'OPTIONS'],
    headers: ['content-type', 'authorization'],
    credentials: false,
    maxAge: 600,
));
```

As origens são explícitas; a lista não pode ser vazia.

## Cabeçalhos de segurança

`SecurityHeadersMiddleware` aplica cabeçalhos conservadores de navegador (o template web registra por padrão):

```php
$app->middleware(new Nimbus\Middleware\SecurityHeadersMiddleware());
```

## Limites e prazos

- Corpo da requisição: opção `maxBodyBytes` do `listen()` (2 MiB por padrão).
- Prazo por requisição: `DeadlineMiddleware` responde `504` (`Timeout`) quando o prazo cooperativo estoura.
- Rate limit: [Redis, cache e rate limit](./redis.md#rate-limit).
- Os stores em memória (rate limit, idempotência, cache, revogação de token) têm tamanho máximo, para um cliente não crescer a memória do worker sem limite.

Vulnerabilidades: reporte pelo GitHub private vulnerability reporting do repositório `push-in/pam`, nunca numa issue pública.
