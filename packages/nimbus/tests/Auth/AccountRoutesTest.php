<?php

declare(strict_types=1);

namespace Nimbus\Tests\Auth;

use Nimbus\App;
use Nimbus\Auth\AuthRoutes;
use Nimbus\Auth\AuthUser;
use Nimbus\Auth\EmailVerification;
use Nimbus\Auth\HmacTokenCodec;
use Nimbus\Auth\OneTimeTokens;
use Nimbus\Auth\PasswordResets;
use Nimbus\Auth\RefreshTokens;
use Nimbus\Auth\ResetPasswordNotification;
use Nimbus\Auth\SendPasswordResetLink;
use Nimbus\Jobs\MemoryJobDispatcher;
use Nimbus\RateLimit\MemoryRateLimitStore;
use Nimbus\RateLimit\SharedTableRateLimitStore;
use Nimbus\Auth\TokenPairIssuer;
use Nimbus\Auth\TokenPrincipal;
use Nimbus\Auth\VerifyEmailNotification;
use Nimbus\Notifications\NotificationFake;
use Nimbus\Notifications\Notifier;
use Nimbus\Security\Hash;
use Nimbus\Testing\TestClient;
use PHPUnit\Framework\Attributes\CoversClass;
use PHPUnit\Framework\TestCase;

#[CoversClass(AuthRoutes::class)]
#[CoversClass(PasswordResets::class)]
#[CoversClass(EmailVerification::class)]
#[CoversClass(OneTimeTokens::class)]
#[CoversClass(ResetPasswordNotification::class)]
#[CoversClass(VerifyEmailNotification::class)]
#[CoversClass(SendPasswordResetLink::class)]
final class AccountRoutesTest extends TestCase
{
    private AuthTestDatabase $database;

    private Clock $clock;

    private OneTimeTokens $tokens;

    private AccountUsers $users;

    private NotificationFake $mail;

    protected function setUp(): void
    {
        $this->database = AuthTestDatabase::create();
        $this->clock = new Clock();
        $this->tokens = new OneTimeTokens($this->database->databases, clock: $this->clock->closure());
        $this->users = new AccountUsers(new AccountUser('u7', 'ada@example.com', Hash::make('old-password')));
        $this->mail = Notifier::fake();
    }

    protected function tearDown(): void
    {
        $this->database->drop();
    }

    public function testOneTimeTokensAreSingleUseExpiringAndPurposeBound(): void
    {
        $token = $this->tokens->create('password-reset', 'u7', 600);
        self::assertNull($this->tokens->consume('email-verification', $token));
        self::assertSame('u7', $this->tokens->consume('password-reset', $token));
        self::assertNull($this->tokens->consume('password-reset', $token));

        $first = $this->tokens->create('password-reset', 'u7', 600);
        $second = $this->tokens->create('password-reset', 'u7', 600);
        self::assertNull($this->tokens->consume('password-reset', $first), 'A new token replaces the previous one.');
        self::assertTrue($this->tokens->createdWithin('password-reset', 'u7', 60));
        $this->clock->now += 600;
        self::assertFalse($this->tokens->createdWithin('password-reset', 'u7', 60));
        self::assertNull($this->tokens->consume('password-reset', $second));
        self::assertSame(0, $this->tokens->prune());

        $stored = $this->database->databases->database()->select()->from('auth_tokens')->count();
        self::assertSame(0, $stored);
    }

    public function testPasswordResetByApi(): void
    {
        $revoked = [];
        $passwords = new PasswordResets(
            $this->users,
            $this->tokens,
            $this->mail,
            'https://app.test/reset?token={token}&email={email}',
            afterReset: static function (AuthUser $user) use (&$revoked): void {
                $revoked[] = $user->authIdentifier();
            },
        );
        $client = new TestClient($this->app(new AuthRoutes(passwords: $passwords)));

        $client->postJson('/auth/password/forgot', ['email' => 'nobody@example.com'])->assertStatus(202);
        self::assertSame([], $this->mail->sent());
        $client->postJson('/auth/password/forgot', ['email' => 'ada@example.com'])->assertStatus(202);
        $client->postJson('/auth/password/forgot', ['email' => 'ada@example.com'])->assertStatus(202);
        $sent = $this->mail->sent(ResetPasswordNotification::class);
        self::assertCount(1, $sent, 'The second request is throttled.');
        self::assertStringContainsString('email=ada%40example.com', $sent[0]->url);
        self::assertStringContainsString('Reset password', $sent[0]->toMail($this->user())->renderText());
        parse_str((string) parse_url($sent[0]->url, PHP_URL_QUERY), $query);
        $token = $query['token'] ?? null;
        self::assertIsString($token);

        $client->postJson('/auth/password/reset', ['token' => $token, 'password' => 'short'])
            ->assertUnprocessableEntity()->assertJsonValidationErrors('password');
        $client->postJson('/auth/password/reset', ['token' => $token, 'password' => 'new-password-123'])->assertNoContent();
        self::assertTrue(Hash::check('new-password-123', $this->user()->passwordHash));
        self::assertSame(['u7'], $revoked);
        $client->postJson('/auth/password/reset', ['token' => $token, 'password' => 'another-password'])
            ->assertUnprocessableEntity()->assertJsonValidationErrors('token');
        $client->postJson('/auth/password/reset', ['password' => 'another-password'])
            ->assertJsonValidationErrors('token');
    }

    public function testEmailVerificationByApi(): void
    {
        $verification = new EmailVerification($this->users, $this->tokens, $this->mail, 'https://app.test/verify/{token}');
        $app = $this->app(new AuthRoutes(verification: $verification));
        $client = (new TestClient($app))->actingAs(new TokenPrincipal('u7', ['*'], 'jti'));

        $client->postJson('/auth/email/resend')->assertStatus(202)->assertJsonPath('verified', false);
        $sent = $this->mail->sent(VerifyEmailNotification::class);
        self::assertCount(1, $sent);
        $token = rawurldecode(substr($sent[0]->url, strlen('https://app.test/verify/')));

        $client->postJson('/auth/email/verify', ['token' => 'bogus'])->assertJsonValidationErrors('token');
        $client->postJson('/auth/email/verify', ['token' => $token])->assertNoContent();
        self::assertTrue($this->user()->verified);
        self::assertFalse($verification->send($this->user()), 'Verified users get no more links.');
    }

    public function testRefreshRoutesRotateAndRevoke(): void
    {
        $codec = new HmacTokenCodec(str_repeat('k', 32), 'iss', 'aud');
        $pairs = new TokenPairIssuer($codec, new RefreshTokens($this->database->databases));
        $client = new TestClient($this->app(new AuthRoutes(tokens: $pairs)));
        $pair = $pairs->issue('u7', ['orders:read']);

        $refreshed = $client->postJson('/auth/token/refresh', ['refresh_token' => $pair->refreshToken])->assertOk();
        self::assertSame('u7', $codec->verify(self::text($refreshed->json('access_token')))?->identifier());
        $client->postJson('/auth/token/refresh', ['refresh_token' => $pair->refreshToken])->assertUnauthorized();
        $client->postJson('/auth/token/refresh', ['refresh_token' => self::text($refreshed->json('refresh_token'))])
            ->assertUnauthorized();

        $other = $pairs->issue('u7');
        $client->postJson('/auth/token/revoke', ['refresh_token' => $other->refreshToken])->assertNoContent();
        $client->postJson('/auth/token/refresh', ['refresh_token' => $other->refreshToken])->assertUnauthorized();
    }

    public function testAuthRoutesAreThrottledPerClient(): void
    {
        $passwords = new PasswordResets($this->users, $this->tokens, $this->mail, 'https://app.test/{token}', revoke: []);
        $client = new TestClient($this->app(new AuthRoutes(passwords: $passwords, throttleBurst: 2)));

        $client->postJson('/auth/password/forgot', ['email' => 'x@example.com'])->assertStatus(202);
        $client->postJson('/auth/password/forgot', ['email' => 'x@example.com'])->assertStatus(202);
        $client->postJson('/auth/password/forgot', ['email' => 'x@example.com'])->assertStatus(429);
    }

    public function testForgotPasswordAnswersAlikeForKnownAndUnknownAddresses(): void
    {
        $passwords = new PasswordResets($this->users, $this->tokens, $this->mail, 'https://app.test/{token}', revoke: []);
        $client = new TestClient($this->app(new AuthRoutes(passwords: $passwords, forgotPasswordMinimumMilliseconds: 50)));

        $started = hrtime(true);
        $unknown = $client->postJson('/auth/password/forgot', ['email' => 'nobody@example.com'])->assertStatus(202);
        self::assertGreaterThanOrEqual(50.0, (hrtime(true) - $started) / 1e6, 'The response is padded to the floor.');
        $known = $client->postJson('/auth/password/forgot', ['email' => 'ada@example.com'])->assertStatus(202);
        self::assertSame($unknown->body(), $known->body());
        self::assertSame($unknown->header('content-type'), $known->header('content-type'));
        self::assertCount(1, $this->mail->sent(ResetPasswordNotification::class));

        // A failing mail transport is not a 500 that only known addresses would get.
        $failing = new PasswordResets(
            $this->users,
            $this->tokens,
            new class implements \Nimbus\Notifications\NotificationDispatcher {
                public function send(
                    \Nimbus\Notifications\Notifiable|iterable $notifiables,
                    \Nimbus\Notifications\Notification $notification,
                ): void {
                    throw new \RuntimeException('SMTP down');
                }

                public function sendLater(
                    \Nimbus\Notifications\Notifiable|iterable $notifiables,
                    \Nimbus\Notifications\Notification $notification,
                    ?int $delaySeconds = null,
                ): array {
                    throw new \RuntimeException('SMTP down');
                }
            },
            'https://app.test/{token}',
            throttleSeconds: 0,
            revoke: [],
        );
        $this->clock->now += 120;
        $client = new TestClient($this->app(new AuthRoutes(passwords: $failing, forgotPasswordMinimumMilliseconds: 0)));
        self::assertSame($unknown->body(), $client->postJson('/auth/password/forgot', ['email' => 'ada@example.com'])->assertStatus(202)->body());
    }

    public function testQueuedForgotPasswordSendsNothingInTheRequest(): void
    {
        $queue = new MemoryJobDispatcher();
        $passwords = new PasswordResets($this->users, $this->tokens, $this->mail, 'https://app.test/{token}', revoke: [], queue: $queue);
        $client = new TestClient($this->app(new AuthRoutes(passwords: $passwords, forgotPasswordMinimumMilliseconds: 0)));

        $client->postJson('/auth/password/forgot', ['email' => 'ada@example.com'])->assertStatus(202);
        $client->postJson('/auth/password/forgot', ['email' => 'nobody@example.com'])->assertStatus(202);
        self::assertSame([], $this->mail->sent(), 'No mail I/O or user lookup happens in the request.');

        $jobs = $queue->pending();
        self::assertCount(2, $jobs);
        foreach ($jobs as $envelope) {
            $job = $envelope->job;
            self::assertInstanceOf(SendPasswordResetLink::class, $job);
            self::assertSame(['email'], array_keys($job->toJobPayload()), 'The token never enters the payload.');
            SendPasswordResetLink::fromJobPayload($job->toJobPayload())->handle($passwords);
        }
        self::assertCount(1, $this->mail->sent(ResetPasswordNotification::class));
    }

    public function testAVerificationLinkOnlyVerifiesTheAddressItWasSentTo(): void
    {
        $verification = new EmailVerification($this->users, $this->tokens, $this->mail, 'https://app.test/verify/{token}');
        self::assertTrue($verification->send($this->user()));
        $sent = $this->mail->sent(VerifyEmailNotification::class);
        $token = rawurldecode(substr($sent[0]->url, strlen('https://app.test/verify/')));

        $this->user()->email = 'ceo@example.com';
        self::assertFalse($verification->verify($token), 'The link of the old address does not verify the new one.');
        self::assertFalse($this->user()->verified);

        $this->clock->now += 120;
        self::assertTrue($verification->send($this->user()));
        $sent = $this->mail->sent(VerifyEmailNotification::class);
        $token = rawurldecode(substr($sent[1]->url, strlen('https://app.test/verify/')));
        self::assertTrue($verification->verify($token));
        self::assertTrue($this->user()->verified);
    }

    public function testTheAuthThrottleIsSharedByEveryWorkerUnderPam(): void
    {
        self::assertInstanceOf(MemoryRateLimitStore::class, AuthRoutes::defaultThrottleStore());
        \Pam\Shared\Table::$runtime = true;
        try {
            self::assertInstanceOf(SharedTableRateLimitStore::class, AuthRoutes::defaultThrottleStore());
        } finally {
            \Pam\Shared\Table::$runtime = false;
        }
    }

    private static function text(mixed $value): string
    {
        self::assertIsString($value);
        return $value;
    }

    private function user(): AccountUser
    {
        return $this->users->users['u7'] ?? throw new \LogicException('The fixture user is missing.');
    }

    private function app(AuthRoutes $routes): App
    {
        $app = new App(discoverPackages: false);
        $routes->register($app);
        return $app;
    }
}
