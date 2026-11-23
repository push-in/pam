<?php

declare(strict_types=1);

namespace Nimbus\Auth;

use Nimbus\App;
use Nimbus\Auth\OAuth\SocialLogin;
use Nimbus\Http\ClientIpResolver;
use Nimbus\Http\HttpException;
use Nimbus\Http\ProblemCode;
use Nimbus\Middleware\RateLimitMiddleware;
use Nimbus\RateLimit\MemoryRateLimitStore;
use Nimbus\RateLimit\RateLimitStore;
use Nimbus\RateLimit\SharedTableRateLimitStore;
use Nimbus\Validation\ValidationException;
use Pam\Http\Request;
use Pam\Http\Response;

/**
 * The account routes of an API, registered with `register($app, '/auth')` for the services given:
 *
 * - `POST /password/forgot` (`email`) and `POST /password/reset` (`token`, `password`);
 * - `POST /email/verify` (`token`) and `POST /email/resend` (authenticated);
 * - `POST /token/refresh` and `POST /token/revoke` (`refresh_token`);
 * - `GET /oauth/{provider}` and `/oauth/{provider}/callback` of a `SocialLogin`.
 *
 * Every route is throttled per client IP through a `RateLimitMiddleware`. Without a
 * `$throttleStore`, the limit lives in a PAM shared table, which every worker of the server
 * shares; outside PAM it falls back to one in-memory store per process and, in production, warns
 * once (pass a `RedisRateLimitStore` to share the limit across machines). The reset and
 * verification flows also throttle each user's e-mails.
 *
 * `POST /password/forgot` answers the same 202 whether or not the address exists, and never
 * sooner than `$forgotPasswordMinimumMilliseconds`: with `PasswordResets(queue: ...)` the e-mail
 * leaves the request entirely, and the floor hides the account lookup; without a queue it also
 * hides the mail transport up to that time.
 */
final readonly class AuthRoutes
{
    public function __construct(
        private ?PasswordResets $passwords = null,
        private ?EmailVerification $verification = null,
        private ?TokenPairIssuer $tokens = null,
        private ?SocialLogin $social = null,
        private ?RateLimitStore $throttleStore = null,
        private int $throttleBurst = 10,
        private ClientIpResolver $clientIp = new ClientIpResolver(),
        private int $forgotPasswordMinimumMilliseconds = 250,
    ) {
        if ($throttleBurst < 1) {
            throw new \InvalidArgumentException('The auth throttle burst must be positive.');
        }
        if ($forgotPasswordMinimumMilliseconds < 0 || $forgotPasswordMinimumMilliseconds > 10_000) {
            throw new \InvalidArgumentException('The forgot-password response floor must be 0 to 10,000 ms.');
        }
    }

    /**
     * The throttle store used when none is given: the PAM shared table, shared by every worker,
     * or a per-process memory store with a production warning.
     */
    public static function defaultThrottleStore(): RateLimitStore
    {
        if (SharedTableRateLimitStore::available()) {
            return new SharedTableRateLimitStore(name: 'nimbus_auth_throttle');
        }
        static $warned = false;
        $environment = getenv('APP_ENV') ?: getenv('PAM_ENV') ?: '';
        if (!$warned && strtolower($environment) === 'production') {
            $warned = true;
            error_log('Nimbus AuthRoutes: the auth throttle is per process (no PAM shared table); '
                . 'pass a RedisRateLimitStore as $throttleStore so every worker shares the limit.');
        }
        return new MemoryRateLimitStore();
    }

    public function register(App $app, string $path = '/auth'): void
    {
        $path = rtrim('/' . trim($path, '/'), '/');
        $throttle = new RateLimitMiddleware(
            requestsPerSecond: 1,
            burst: $this->throttleBurst,
            store: $this->throttleStore ?? self::defaultThrottleStore(),
            keyResolver: fn (Request $request): string => 'auth:' . $this->clientIp->resolve($request),
        );
        if ($this->passwords !== null) {
            $app->post($path . '/password/forgot', $this->forgotPassword(...))
                ->middleware($throttle)->summary('Send a password reset link')->tags(['auth']);
            $app->post($path . '/password/reset', $this->resetPassword(...))
                ->middleware($throttle)->summary('Reset the password with an e-mailed token')->tags(['auth']);
        }
        if ($this->verification !== null) {
            $app->post($path . '/email/verify', $this->verifyEmail(...))
                ->middleware($throttle)->summary('Verify the e-mail address with an e-mailed token')->tags(['auth']);
            $app->post($path . '/email/resend', $this->resendVerification(...))
                ->auth()->middleware($throttle)->summary('Send the e-mail verification link again')->tags(['auth']);
        }
        if ($this->tokens !== null) {
            $app->post($path . '/token/refresh', $this->refresh(...))
                ->middleware($throttle)->summary('Rotate the refresh token and issue a new access token')->tags(['auth']);
            $app->post($path . '/token/revoke', $this->revoke(...))
                ->middleware($throttle)->summary('Revoke a refresh token family')->tags(['auth']);
        }
        $this->social?->routes($app, $path . '/oauth', [$throttle]);
    }

    public function forgotPassword(Request $request, Response $response): Response
    {
        $started = hrtime(true);
        $email = AuthInput::string($request, 'email', 254);
        try {
            $this->passwordResets()->requestResetLink($email);
        } catch (\Throwable $error) {
            // A failing mail transport or queue must not reveal that the address exists.
            error_log('Nimbus AuthRoutes: the password reset link could not be sent (' . $error::class . ').');
        }
        self::waitUntil($started, $this->forgotPasswordMinimumMilliseconds);
        // The same answer whether or not the address exists.
        return $response->json(['message' => 'If the address belongs to an account, a reset link was sent.'], 202);
    }

    public function resetPassword(Request $request, Response $response): Response
    {
        $token = AuthInput::string($request, 'token', 256);
        $password = AuthInput::string($request, 'password');
        try {
            $reset = $this->passwordResets()->reset($token, $password);
        } catch (\InvalidArgumentException $invalid) {
            throw new ValidationException(['password' => [$invalid->getMessage()]]);
        }
        if (!$reset) {
            throw new ValidationException(['token' => ['The reset token is invalid or expired.']]);
        }
        return $response->status(204);
    }

    public function verifyEmail(Request $request, Response $response): Response
    {
        if (!$this->emailVerification()->verify(AuthInput::string($request, 'token', 256))) {
            throw new ValidationException(['token' => ['The verification token is invalid or expired.']]);
        }
        return $response->status(204);
    }

    public function resendVerification(Request $request, Response $response, Principal $principal): Response
    {
        $verification = $this->emailVerification();
        $user = $verification->users()->findById($principal->identifier());
        if ($user === null) {
            throw new HttpException(404, ProblemCode::NotFound, 'User not found.');
        }
        $verification->send($user);
        return $response->json(['verified' => $user->hasVerifiedEmail()], 202);
    }

    public function refresh(Request $request, Response $response): Response
    {
        return $response->json($this->tokenPairs()->refresh(AuthInput::string($request, 'refresh_token', 256))->toArray());
    }

    public function revoke(Request $request, Response $response): Response
    {
        $this->tokenPairs()->revoke(AuthInput::string($request, 'refresh_token', 256));
        return $response->status(204);
    }

    /** Pads the response to at least `$milliseconds` after `$started`, yielding the Fiber under PAM. */
    private static function waitUntil(int|float $started, int $milliseconds): void
    {
        $remaining = $milliseconds / 1_000 - (hrtime(true) - $started) / 1e9;
        if ($remaining <= 0) {
            return;
        }
        if (\Fiber::getCurrent() !== null && function_exists('Pam\\Async\\delay')) {
            \Pam\Async\delay($remaining);
            return;
        }
        usleep((int) ($remaining * 1_000_000));
    }

    private function passwordResets(): PasswordResets
    {
        return $this->passwords ?? throw new \LogicException('AuthRoutes has no PasswordResets.');
    }

    private function emailVerification(): EmailVerification
    {
        return $this->verification ?? throw new \LogicException('AuthRoutes has no EmailVerification.');
    }

    private function tokenPairs(): TokenPairIssuer
    {
        return $this->tokens ?? throw new \LogicException('AuthRoutes has no TokenPairIssuer.');
    }
}
