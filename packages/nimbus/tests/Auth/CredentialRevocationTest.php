<?php

declare(strict_types=1);

namespace Nimbus\Tests\Auth;

use Nimbus\Auth\AuthUser;
use Nimbus\Auth\OneTimeTokens;
use Nimbus\Auth\PasswordResets;
use Nimbus\Auth\PersonalAccessTokens;
use Nimbus\Auth\Principal;
use Nimbus\Auth\RefreshTokens;
use Nimbus\Auth\RevokesCredentials;
use Nimbus\Auth\SessionAuthenticator;
use Nimbus\Auth\SessionRevocations;
use Nimbus\Auth\TokenPrincipal;
use Nimbus\Container\Container;
use Nimbus\Notifications\NotificationDispatcher;
use Nimbus\Security\Hash;
use Nimbus\Session\Session;
use Pam\Http\Request;
use PHPUnit\Framework\Attributes\CoversClass;
use PHPUnit\Framework\TestCase;

#[CoversClass(PasswordResets::class)]
#[CoversClass(SessionRevocations::class)]
#[CoversClass(SessionAuthenticator::class)]
final class CredentialRevocationTest extends TestCase
{
    private AuthTestDatabase $database;

    private int $now = 1_800_000_000;

    protected function setUp(): void
    {
        $this->database = AuthTestDatabase::create();
    }

    protected function tearDown(): void
    {
        $this->database->drop();
    }

    public function testPasswordResetsMustSayWhatTheyRevoke(): void
    {
        $this->expectException(\InvalidArgumentException::class);
        $this->expectExceptionMessage('revoke');
        new PasswordResets(
            new AccountUsers(),
            new OneTimeTokens($this->database->databases),
            $this->createStub(NotificationDispatcher::class),
            'https://app.test/reset?token={token}',
        );
    }

    public function testAResetRevokesSessionsAndTokensOfTheUser(): void
    {
        $clock = fn (): int => $this->now;
        $oneTime = new OneTimeTokens($this->database->databases, clock: $clock);
        $refresh = new RefreshTokens($this->database->databases, clock: $clock);
        $personal = new PersonalAccessTokens($this->database->databases, clock: $clock);
        $sessions = new SessionRevocations($oneTime);
        $users = new AccountUsers(new AccountUser('u7', 'ada@example.com', Hash::make('old-password')));
        $user = $users->findByEmail('ada@example.com');
        self::assertInstanceOf(AuthUser::class, $user);
        $subject = $user->authIdentifier();

        // A session that logged in before the reset (the attacker's).
        $container = new Container();
        $authenticator = new SessionAuthenticator(
            $container,
            static fn (string $id): ?Principal => new TokenPrincipal($id, [], 'session'),
            $sessions,
            $clock,
        );
        $session = new Session(Session::newId());
        $container->beginScope([Session::class => $session]);
        $authenticator->login(new TokenPrincipal($subject, [], 'session'));
        $request = new Request('GET', '/', [], [], '');
        self::assertNotNull($authenticator->authenticate($request));
        $refresh->issue($subject);
        $pat = $personal->create($subject, 'cli');

        $this->now += 10;
        $passwords = new PasswordResets(
            $users,
            $oneTime,
            $this->createStub(NotificationDispatcher::class),
            'https://app.test/reset?token={token}',
            revoke: [$refresh, $personal, $sessions],
        );
        $token = $oneTime->create(PasswordResets::PURPOSE, $subject, 600);
        self::assertTrue($passwords->reset($token, 'a-new-long-password'));

        self::assertNull($authenticator->authenticate($request), 'The old session is signed out.');
        self::assertNull($personal->find($pat->plainTextToken));
        self::assertSame(0, $refresh->revokeAll($subject), 'Refresh tokens were already revoked.');

        // A login after the reset works.
        $this->now += 1;
        $authenticator->login(new TokenPrincipal($subject, [], 'session'));
        self::assertNotNull($authenticator->authenticate($request));
        $container->endScope();
        self::assertInstanceOf(RevokesCredentials::class, $sessions);
    }
}
