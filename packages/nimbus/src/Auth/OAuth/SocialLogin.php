<?php

declare(strict_types=1);

namespace Nimbus\Auth\OAuth;

use League\OAuth2\Client\Provider\AbstractProvider;
use League\OAuth2\Client\Provider\Exception\IdentityProviderException;
use Nimbus\App;
use Nimbus\Auth\Abilities;
use Nimbus\Auth\AuthInput;
use Nimbus\Auth\LoginTokenIssuer;
use Nimbus\Http\Cookie;
use Nimbus\Http\HttpException;
use Nimbus\Http\ProblemCode;
use Nimbus\Http\RequestCookies;
use Pam\Contracts\Http\MiddlewareInterface;
use Pam\Http\Request;
use Pam\Http\Response;

/**
 * Social login for APIs on league/oauth2-client (optional dependency), with any provider
 * package (league/oauth2-google, league/oauth2-github, ...) or `GenericProvider`.
 *
 * `authorizationUrl()` returns the provider URL with a fresh state and a PKCE (S256) challenge;
 * the state and verifier stay on the server in an `OAuthStateStore`. `callback()` spends the
 * state, exchanges the code, lets the application's `SocialUserResolver` find or create the
 * user and returns a Nimbus token from the `LoginTokenIssuer`.
 *
 * The state is bound to the client that started the flow: the start route sets an HttpOnly,
 * SameSite=Lax cookie with a random secret whose hash is stored with the state, and the callback
 * refuses a state presented without it. Someone who starts a login with their own account cannot
 * make a victim finish it (login CSRF). `bindStateToClient: false` turns this off for clients
 * that cannot keep cookies between the two calls.
 */
final readonly class SocialLogin
{
    /** @var array<string, AbstractProvider|\Closure(): AbstractProvider> */
    private array $providers;

    /** @var list<string> */
    private array $abilities;

    /** @var \Closure(): int */
    private \Closure $clock;

    public const string BINDING_COOKIE = 'nimbus_oauth_state';

    /**
     * @param array<string, AbstractProvider|\Closure(): AbstractProvider> $providers By name, e.g. `google`.
     * @param list<string> $abilities Abilities of the issued tokens.
     * @param (\Closure(): int)|null $clock
     */
    public function __construct(
        array $providers,
        private OAuthStateStore $states,
        private SocialUserResolver $users,
        private LoginTokenIssuer $tokens,
        array $abilities = ['*'],
        private int $stateTtlSeconds = 600,
        ?\Closure $clock = null,
        private bool $bindStateToClient = true,
        private bool $secureCookie = true,
    ) {
        if (!class_exists(AbstractProvider::class)) {
            throw new \LogicException('Social login requires league/oauth2-client: composer require league/oauth2-client');
        }
        foreach (array_keys($providers) as $name) {
            if (preg_match('/^[a-z0-9][a-z0-9_-]{0,31}$/D', $name) !== 1) {
                throw new \InvalidArgumentException('OAuth provider names are short lowercase identifiers.');
            }
        }
        if ($stateTtlSeconds < 60 || $stateTtlSeconds > 3_600) {
            throw new \InvalidArgumentException('The OAuth state TTL must be between one minute and one hour.');
        }
        $this->providers = $providers;
        $this->abilities = Abilities::normalize($abilities);
        $this->clock = $clock ?? time(...);
    }

    /** @return list<string> */
    public function providers(): array
    {
        return array_keys($this->providers);
    }

    /**
     * @param list<string> $scopes Provider scopes; the provider's defaults when empty.
     * @param string|null $binding Secret the callback must present again (the binding cookie).
     * @return array{authorization_url: string, state: string, expires_in: int}
     */
    public function authorizationUrl(string $provider, array $scopes = [], #[\SensitiveParameter] ?string $binding = null): array
    {
        $client = $this->provider($provider);
        $state = bin2hex(random_bytes(32));
        $verifier = rtrim(strtr(base64_encode(random_bytes(48)), '+/', '-_'), '=');
        $options = [
            'state' => $state,
            'code_challenge' => rtrim(strtr(base64_encode(hash('sha256', $verifier, true)), '+/', '-_'), '='),
            'code_challenge_method' => 'S256',
        ];
        if ($scopes !== []) {
            $options['scope'] = $scopes;
        }
        $url = $client->getAuthorizationUrl($options);
        // Providers configured with their own PKCE method replace the challenge; keep their verifier.
        $own = $client->getPkceCode();
        if (is_string($own) && $own !== '') {
            $verifier = $own;
        }
        $data = ['provider' => $provider, 'verifier' => $verifier];
        if ($binding !== null) {
            $data['binding'] = hash('sha256', $binding);
        }
        $this->states->put($state, $data, ($this->clock)() + $this->stateTtlSeconds);
        return ['authorization_url' => $url, 'state' => $state, 'expires_in' => $this->stateTtlSeconds];
    }

    /**
     * @return array<string, mixed> The token response of the `LoginTokenIssuer`.
     * @throws HttpException 404 for an unknown provider, 422 for an invalid state, 401 when the provider refuses the code.
     */
    public function callback(
        string $provider,
        string $state,
        #[\SensitiveParameter] string $code,
        #[\SensitiveParameter] ?string $binding = null,
    ): array {
        $client = $this->provider($provider);
        $pending = $this->states->pull($state, ($this->clock)());
        $bound = !isset($pending['binding'])
            || ($binding !== null && hash_equals($pending['binding'], hash('sha256', $binding)));
        if ($pending === null || !hash_equals($pending['provider'] ?? '', $provider) || !$bound) {
            throw new HttpException(422, ProblemCode::ValidationFailed, 'The OAuth state is invalid or expired.', [
                'errors' => ['state' => ['The OAuth state is invalid or expired.']],
            ]);
        }
        $verifier = $pending['verifier'] ?? '';
        if ($verifier !== '') {
            $client->setPkceCode($verifier);
        }
        try {
            $token = $client->getAccessToken('authorization_code', ['code' => $code]);
            if (!$token instanceof \League\OAuth2\Client\Token\AccessToken) {
                throw new \UnexpectedValueException('The provider returned an unsupported access token.');
            }
            $identity = SocialIdentity::fromResourceOwner($provider, $client->getResourceOwner($token));
        } catch (IdentityProviderException | \UnexpectedValueException | \InvalidArgumentException | \RuntimeException $error) {
            if ($error instanceof HttpException) {
                throw $error;
            }
            throw new HttpException(401, ProblemCode::Unauthenticated, 'The provider did not accept the authorization.', [], $error);
        }
        return $this->tokens->issueLoginToken($this->users->resolve($identity), $this->abilities);
    }

    /**
     * Registers `GET {path}/{provider}` (authorization URL) and the callback as
     * `POST {path}/{provider}/callback` (JSON `code` and `state`) and `GET` (query string).
     *
     * @param list<MiddlewareInterface|callable> $middleware E.g. a RateLimitMiddleware.
     */
    public function routes(App $app, string $path = '/auth/oauth', array $middleware = []): void
    {
        $path = rtrim('/' . trim($path, '/'), '/');
        $cookiePath = $path . '/';
        $redirect = $app->get($path . '/{provider}', function (Request $request, Response $response) use ($cookiePath): Response {
            $binding = null;
            if ($this->bindStateToClient) {
                $binding = bin2hex(random_bytes(32));
                $response->addHeader('set-cookie', (new Cookie(
                    self::BINDING_COOKIE,
                    $binding,
                    $this->stateTtlSeconds,
                    $cookiePath,
                    secure: $this->secureCookie,
                ))->toHeader());
            }
            return $response->json($this->authorizationUrl((string) $request->route('provider'), binding: $binding));
        })->summary('Start a social login')->tags(['auth']);
        $callback = function (Request $request, Response $response) use ($cookiePath): Response {
            $binding = RequestCookies::fromRequest($request)->get(self::BINDING_COOKIE);
            $result = $this->callback(
                (string) $request->route('provider'),
                AuthInput::string($request, 'state', 256),
                AuthInput::string($request, 'code', 2_048),
                $binding,
            );
            if ($binding !== null) {
                $response->addHeader('set-cookie', Cookie::forget(self::BINDING_COOKIE, $cookiePath)->toHeader());
            }
            return $response->json($result);
        };
        $post = $app->post($path . '/{provider}/callback', $callback)->summary('Finish a social login')->tags(['auth']);
        $get = $app->get($path . '/{provider}/callback', $callback)->summary('Finish a social login (redirect)')->tags(['auth']);
        foreach ($middleware as $layer) {
            $redirect->middleware($layer);
            $post->middleware($layer);
            $get->middleware($layer);
        }
    }

    private function provider(string $name): AbstractProvider
    {
        $provider = $this->providers[$name] ?? null;
        if ($provider instanceof \Closure) {
            $provider = $provider();
        }
        if (!$provider instanceof AbstractProvider) {
            throw new HttpException(404, ProblemCode::NotFound, 'Unknown OAuth provider.');
        }
        // Providers keep the state and PKCE code of the last call: each request uses its own copy.
        return clone $provider;
    }
}
