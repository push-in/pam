<?php

declare(strict_types=1);

namespace Nimbus\Tests\Session;

use Nimbus\App;
use Nimbus\Auth\Authenticator;
use Nimbus\Auth\Principal;
use Nimbus\Auth\SessionAuthenticator;
use Nimbus\Http\Cookie;
use Nimbus\Http\CookieCodec;
use Nimbus\Http\CookieJar;
use Nimbus\Http\FormData;
use Nimbus\Http\Redirect;
use Nimbus\Http\RequestCookies;
use Nimbus\Middleware\EncryptCookiesMiddleware;
use Nimbus\Middleware\MethodOverrideMiddleware;
use Nimbus\Middleware\StartSessionMiddleware;
use Nimbus\Middleware\VerifyCsrfTokenMiddleware;
use Nimbus\Security\Crypt;
use Nimbus\Session\MemorySessionStore;
use Nimbus\Session\Session;
use Nimbus\Session\SessionConfig;
use Nimbus\Session\SessionServiceProvider;
use Nimbus\Testing\TestClient;
use Nimbus\Testing\TestResponse;
use Nimbus\Validation\ValidationException;
use Nimbus\View\ViewServiceProvider;
use Pam\Http\Request;
use Pam\Http\Response;
use PHPUnit\Framework\Attributes\CoversClass;
use PHPUnit\Framework\TestCase;

#[CoversClass(StartSessionMiddleware::class)]
#[CoversClass(VerifyCsrfTokenMiddleware::class)]
#[CoversClass(EncryptCookiesMiddleware::class)]
#[CoversClass(MethodOverrideMiddleware::class)]
#[CoversClass(SessionServiceProvider::class)]
#[CoversClass(SessionAuthenticator::class)]
#[CoversClass(Cookie::class)]
#[CoversClass(CookieCodec::class)]
#[CoversClass(CookieJar::class)]
#[CoversClass(RequestCookies::class)]
#[CoversClass(FormData::class)]
#[CoversClass(Redirect::class)]
#[CoversClass(SessionConfig::class)]
final class WebFlowTest extends TestCase
{
    private MemorySessionStore $store;

    private CookieCodec $codec;

    protected function setUp(): void
    {
        $this->store = new MemorySessionStore();
        $key = random_bytes(32);
        $this->codec = new CookieCodec(new Crypt($key), $key);
    }

    public function testApiRequestsThatNeverTouchTheSessionWriteNothing(): void
    {
        $client = $this->client();

        $response = $client->get('/ping')->assertOk();

        self::assertNull($response->header('set-cookie'));
    }

    public function testCsrfProtectsStateChangingRequests(): void
    {
        $client = $this->client();
        $form = $client->get('/form')->assertOk();
        $session = $form->cookie('nimbus_session');
        self::assertIsString($session);
        $token = self::token($form);
        self::assertSame($token, $form->cookie('XSRF-TOKEN'));
        $browser = $client->withCookiesFrom($form);

        $browser->post('/submit', ['title' => 'x'])->assertStatus(419)->assertJsonPath('code', 13);
        $browser->post('/submit', ['title' => 'x', '_token' => 'wrong'])->assertStatus(419);
        $client->post('/submit', ['title' => 'x', '_token' => $token])->assertStatus(419);
        $browser->post('/submit', ['title' => 'ok', '_token' => $token])->assertOk()->assertSee('saved ok');
        $browser->withHeader('x-csrf-token', $token)->postJson('/submit', ['title' => 'json'])->assertOk();
        $browser->withHeader('x-xsrf-token', $token)->postJson('/submit', ['title' => 'xsrf'])->assertOk();
        $client->withToken('api-token')->postJson('/submit', ['title' => 'bearer'])->assertOk();
        $client->postJson('/webhooks/stripe', [])->assertOk();
    }

    public function testFlashDataValidationRedirectsAndOldInput(): void
    {
        $client = $this->client();
        $form = $client->get('/form');
        $token = self::token($form);
        $browser = $client->withCookiesFrom($form);

        $invalid = $browser->post('/validate', ['title' => 'Too short', 'password' => 'secret', '_token' => $token], ['referer' => 'http://app.test/form', 'host' => 'app.test'])
            ->assertRedirect('/form');
        $browser = $browser->withCookiesFrom($invalid);
        $page = $browser->get('/form')->assertOk()
            ->assertSee('<li>The title is invalid.</li>', false)
            ->assertSee('value="Too short"', false);
        self::assertStringNotContainsString('secret', $page->body());
        $browser->get('/form')->assertDontSee('The title is invalid.');

        $browser->withHeader('x-csrf-token', $token)->postJson('/validate', ['title' => 'x'])
            ->assertUnprocessableEntity();

        $browser->get('/flash')->assertOk();
        $browser->get('/read-flash')->assertSee('flashed');
        $browser->get('/read-flash')->assertDontSee('flashed');
    }

    public function testLoginRegeneratesTheSessionAndLogoutInvalidatesIt(): void
    {
        $client = $this->client();
        $form = $client->get('/form');
        $before = $form->cookie('nimbus_session');
        $token = self::token($form);
        $browser = $client->withCookiesFrom($form);
        $browser->get('/form')->assertSee('guest');

        $login = $browser->post('/login', ['_token' => $token])->assertRedirect('/dashboard');
        $after = $login->cookie('nimbus_session');
        self::assertNotNull($after);
        self::assertNotSame($before, $after);
        self::assertNull($this->store->read((string) $before), 'The pre-login session id must be destroyed.');
        $browser = $browser->withCookiesFrom($login);
        $browser->get('/form')->assertSee('signed in')->assertDontSee('<p>guest</p>', false);
        $browser->get('/me')->assertOk()->assertJsonPath('id', 'user-1');

        $client->withCookies(['nimbus_session' => (string) $before])->get('/form')->assertSee('guest');
    }

    public function testABearerHeaderDoesNotLetTheSessionCookieSkipCsrf(): void
    {
        $client = $this->client();
        $form = $client->get('/form');
        $token = self::token($form);
        $login = $client->withCookiesFrom($form)->post('/login', ['_token' => $token]);
        $browser = $client->withCookiesFrom($login);

        // A cross-site request carries the victim's cookie; an invented bearer token must not
        // turn the CSRF check off and then authenticate through that cookie.
        $browser->withToken('forged')->post('/account/delete', [])->assertStatus(401);
        $browser->post('/account/delete', [])->assertStatus(419);
    }

    public function testMethodOverrideAndEncryptedCookies(): void
    {
        $client = $this->client();
        $form = $client->get('/form');
        $token = self::token($form);
        $browser = $client->withCookiesFrom($form);

        $browser->post('/items/5', ['_method' => 'PUT', '_token' => $token])->assertOk()->assertSee('put 5');
        $browser->post('/items/5', ['_method' => 'GET', '_token' => $token])->assertStatus(405);

        $set = $client->get('/cookie/set')->assertOk();
        $encrypted = $set->cookie('theme');
        self::assertNotNull($encrypted);
        self::assertStringStartsWith('v1.', $encrypted);
        self::assertStringContainsString('HttpOnly; SameSite=Lax', (string) $set->header('set-cookie'));
        $client->withCookiesFrom($set)->get('/cookie/get')->assertSee('dark');
        $client->withCookies(['theme' => 'dark'])->get('/cookie/get')->assertSee('none');
        $client->withCookies(['theme' => $this->codec->encrypt('other', 'dark')])->get('/cookie/get')->assertSee('none');
    }

    public function testCookieCodecAndHeaders(): void
    {
        self::assertSame('dark', $this->codec->decrypt('theme', $this->codec->encrypt('theme', 'dark')));
        self::assertNull($this->codec->decrypt('theme', 'garbage'));
        $signed = $this->codec->sign('pref', 'compact');
        self::assertStringStartsWith('compact.', $signed);
        self::assertSame('compact', $this->codec->verify('pref', $signed));
        self::assertNull($this->codec->verify('other', $signed));
        self::assertNull($this->codec->verify('pref', 'compact.forged'));

        self::assertSame(
            'a=b%20c; Expires=Thu, 01 Jan 1970 00:01:40 GMT; Max-Age=100; Path=/; Domain=app.test; Secure; HttpOnly; SameSite=Strict',
            (new Cookie('a', 'b c', 100, '/', 'app.test', sameSite: 'Strict'))->toHeader(0),
        );
        self::assertStringContainsString('Max-Age=0', Cookie::forget('a')->toHeader());
        self::assertSame(['a' => '1', 'b' => 'x y'], RequestCookies::parse('a=1; b="x%20y"; a=2; broken'));

        $this->expectException(\InvalidArgumentException::class);
        new Cookie('a', 'b', sameSite: 'None', secure: false);
    }

    public function testRedirectBackIgnoresForeignReferers(): void
    {
        $request = new Request('POST', '/x', [], ['referer' => ['https://evil.test/a'], 'host' => ['app.test']], '');
        self::assertSame('/fallback', Redirect::back($request, '/fallback')->location);
        $local = new Request('POST', '/x', [], ['referer' => ['https://app.test/a?b=1'], 'host' => ['app.test']], '');
        self::assertSame('/a?b=1', Redirect::back($local)->location);
    }

    public function testRedirectBackNeverLeavesTheHostThroughAProtocolRelativePath(): void
    {
        foreach (['https://app.test//evil.test/a', 'https://app.test/\\evil.test/a'] as $referer) {
            $request = new Request('POST', '/x', [], ['referer' => [$referer], 'host' => ['app.test']], '');
            self::assertSame('/fallback', Redirect::back($request, '/fallback')->location, $referer);
        }
    }

    public function testCsrfExceptionsMatchWholePathSegments(): void
    {
        $app = new App(discoverPackages: false);
        $app->provider(new SessionServiceProvider($this->store, cookies: $this->codec, csrfExcept: ['/webhooks*', '/hooks/*']));
        foreach (['/webhooks', '/webhooks/stripe', '/webhooksX', '/hooks/a', '/hooks'] as $path) {
            $app->post($path, static fn (): string => 'hook');
        }
        $client = new TestClient($app);

        $client->postJson('/webhooks', [])->assertOk();
        $client->postJson('/webhooks/stripe', [])->assertOk();
        $client->postJson('/hooks/a', [])->assertOk();
        $client->postJson('/webhooksX', [])->assertStatus(419);
        $client->postJson('/hooks', [])->assertStatus(419);
    }

    public function testMultipartFormFields(): void
    {
        $body = "--B\r\nContent-Disposition: form-data; name=\"_token\"\r\n\r\nabc\r\n"
            . "--B\r\nContent-Disposition: form-data; name=\"file\"; filename=\"a.txt\"\r\n\r\nbytes\r\n--B--\r\n";
        $request = new Request('POST', '/', [], ['content-type' => ['multipart/form-data; boundary=B']], $body);

        self::assertSame(['_token' => 'abc'], FormData::parse($request));
    }

    private function client(): TestClient
    {
        $app = new App(discoverPackages: false);
        $app->provider(new SessionServiceProvider($this->store, cookies: $this->codec, csrfExcept: ['/webhooks/*']));
        $app->provider(new ViewServiceProvider(dirname(__DIR__) . '/Fixtures/views', 'Nimbus\\Tests\\Fixtures\\ViewComponents'));
        $container = $app->container();
        $sessions = new SessionAuthenticator($container, static fn (string $id): ?Principal => $id === 'user-1' ? new WebUser('user-1') : null);
        $container->instance(Authenticator::class, $sessions);
        $container->instance(SessionAuthenticator::class, $sessions);

        $app->get('/ping', static fn (Response $response): Response => $response->json(['ok' => true]));
        $app->get('/form', static fn () => view('pages.form'));
        $app->post('/submit', static fn (): string => 'saved ok');
        $app->post('/webhooks/stripe', static fn (): string => 'hook');
        $app->post('/validate', static function (): never {
            throw new ValidationException(['title' => ['The title is invalid.']]);
        });
        $app->get('/flash', static function (Session $session): string {
            $session->flash('status', 'flashed');
            return 'ok';
        });
        $app->get('/read-flash', static fn (Session $session): string => is_string($status = $session->get('status')) ? $status : 'nothing');
        $app->post('/login', static function (SessionAuthenticator $auth): Redirect {
            $auth->login(new WebUser('user-1'));
            return Redirect::to('/dashboard');
        });
        $app->get('/me', static fn (Principal $principal, Response $response): Response => $response->json(['id' => $principal->identifier()]))->auth();
        $app->post('/account/delete', static fn (Principal $principal): string => 'deleted ' . $principal->identifier())->auth();
        $app->put('/items/{id}', static fn (Request $request): string => 'put ' . $request->route('id'));
        $app->get('/cookie/set', static function (CookieJar $cookies): string {
            $cookies->make('theme', 'dark', 3600);
            return 'set';
        });
        $app->get('/cookie/get', static fn (RequestCookies $cookies): string => $cookies->get('theme') ?? 'none');
        return new TestClient($app);
    }

    private static function token(TestResponse $response): string
    {
        preg_match('/name="_token" value="([a-f0-9]{40})"/', $response->body(), $match);
        return $match[1] ?? '';
    }
}

final readonly class WebUser implements Principal
{
    public function __construct(private string $id)
    {
    }

    public function identifier(): string
    {
        return $this->id;
    }

    public function can(string $ability): bool
    {
        return $ability === 'publish';
    }
}
