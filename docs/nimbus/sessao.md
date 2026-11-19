# Sessão, cookies e CSRF

Apps de API não precisam de nada desta página. Para páginas renderizadas no servidor, comece pelo template `pam init meu-site --template web`, que já vem com sessão, CSRF, login e views.

## Registrar

```php
use Nimbus\Http\CookieCodec;
use Nimbus\Session\FileSessionStore;
use Nimbus\Session\SessionConfig;
use Nimbus\Session\SessionServiceProvider;

$app->provider(new SessionServiceProvider(
    new FileSessionStore(__DIR__ . '/storage/sessions'),
    new SessionConfig(secure: getenv('SESSION_SECURE_COOKIE') !== 'false'),
    cookies: CookieCodec::fromEnvironment(),
    csrfExcept: ['/webhooks/*'],
));
```

O provider adiciona, nesta ordem: override de método (`_method` ou `X-HTTP-Method-Override`), cookies cifrados, sessão e verificação de CSRF.

`SessionConfig`: `cookie` (`nimbus_session`), `lifetime` (7200 s), `expireOnClose`, `path`, `domain`, `secure` (`true`), `sameSite` (`Lax`) e `lottery` (limpeza de sessões vencidas, 2 em 100 requisições).

## Stores

| Store | Uso |
| --- | --- |
| `FileSessionStore($dir)` | um servidor; padrão do template web |
| `Nimbus\Redis\RedisSessionStore($redis)` | vários workers ou servidores |
| `DatabaseSessionStore($databases)` | tabela `sessions` (`SessionTablesMigration`) |
| `MemorySessionStore` | testes |

A sessão é gravada em JSON (nunca `unserialize()`) e só quando a requisição a usou ou alterou.

## Usar a sessão

Injete `Nimbus\Session\Session`:

```php
use Nimbus\Session\Session;

public function store(Request $request, Session $session): Redirect
{
    $session->put('cart', [42, 43]);
    $cart = $session->get('cart', []);
    $session->forget('cart');
    $coupon = $session->pull('coupon');

    $session->flash('status', 'Pedido criado.');   // disponível só na próxima requisição
    return Redirect::to('/orders');
}
```

Outros métodos: `has()`, `all()`, `flush()`, `now()` (flash só para a requisição atual), `reflash()`, `flashInput()` e `old()`, `flashErrors()` e `errors()` (um `ErrorBag` com `has`, `first`, `get`, `all`), `token()`, `regenerate()` e `invalidate()`.

Uma `ValidationException` numa requisição HTML vira redirect de volta com os erros e o input antigo na sessão, para a view mostrar com `@error` e `old()`.

## Login por sessão

`Nimbus\Auth\SessionAuthenticator` implementa `Authenticator` para páginas:

```php
$this->sessions->login($user);    // grava o usuário e regenera o ID da sessão
$this->sessions->logout();        // invalida a sessão
```

O template web mostra a ligação completa em `app/Providers/AppServiceProvider.php` e `app/Middleware/Authenticate.php` (que manda visitantes para `/login`).

## Redirect

```php
use Nimbus\Http\Redirect;

return Redirect::to('/dashboard');
return Redirect::back($request, fallback: '/');   // só para a mesma origem
```

## Cookies

```php
use Nimbus\Http\CookieJar;
use Nimbus\Http\RequestCookies;

public function show(RequestCookies $cookies, CookieJar $jar, Response $response): Response
{
    $theme = $cookies->get('theme', 'light');
    $jar->make('theme', 'dark', maxAge: 31_536_000);
    $jar->forget('legacy');
    return $response->json(['theme' => $theme]);
}
```

`Cookie` usa por padrão `Secure`, `HttpOnly` e `SameSite=Lax`. `CookieCodec` cifra com `Crypt` (ou assina com HMAC) e amarra o valor ao nome do cookie; `EncryptCookiesMiddleware` aplica o codec a todos os cookies.

## CSRF

`VerifyCsrfTokenMiddleware` responde `419` (`CsrfTokenMismatch`) quando o token não confere. Aceita o campo `_token`, o cabeçalho `X-CSRF-TOKEN` ou `X-XSRF-TOKEN`, e envia o cookie `XSRF-TOKEN` para clientes JavaScript. Ignora `GET`, `HEAD`, `OPTIONS`, requisições com token bearer e os caminhos em `csrfExcept`.

Nos formulários:

```blade
<form method="POST" action="/products">
    @csrf
    @method('PUT')
    ...
</form>
```

`csrf_token()` devolve o token atual.
