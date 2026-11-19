<?php

declare(strict_types=1);

namespace Nimbus\Tests\Auth;

use Nimbus\Auth\DatabaseTokenRevocationStore;
use Nimbus\Auth\HmacTokenCodec;
use Nimbus\Auth\RefreshTokenReusedException;
use Nimbus\Auth\RefreshTokens;
use Nimbus\Auth\TokenPair;
use Nimbus\Auth\TokenPairIssuer;
use Nimbus\Http\HttpException;
use Nimbus\Redis\RedisOAuthStateStore;
use Nimbus\Redis\RedisTokenRevocationStore;
use Nimbus\Tests\Redis\SocketConnection;
use PHPUnit\Framework\Attributes\CoversClass;
use PHPUnit\Framework\TestCase;

#[CoversClass(RefreshTokens::class)]
#[CoversClass(TokenPairIssuer::class)]
#[CoversClass(TokenPair::class)]
#[CoversClass(DatabaseTokenRevocationStore::class)]
#[CoversClass(RedisTokenRevocationStore::class)]
#[CoversClass(RedisOAuthStateStore::class)]
final class RefreshTokensTest extends TestCase
{
    private AuthTestDatabase $database;

    private Clock $clock;

    private RefreshTokens $tokens;

    protected function setUp(): void
    {
        $this->database = AuthTestDatabase::create();
        $this->clock = new Clock();
        $this->tokens = new RefreshTokens($this->database->databases, 3_600, clock: $this->clock->closure());
    }

    protected function tearDown(): void
    {
        $this->database->drop();
    }

    public function testRotationSpendsTheTokenAndKeepsTheFamily(): void
    {
        $first = $this->tokens->issue('user-1', ['orders:read']);
        $second = $this->tokens->rotate($first->plainTextToken);

        self::assertNotNull($second);
        self::assertNotSame($first->plainTextToken, $second->plainTextToken);
        self::assertSame($first->token->familyId, $second->token->familyId);
        self::assertSame(['orders:read'], $second->token->abilities);
        self::assertSame($this->clock->now + 3_600, $second->token->expiresAt);
        self::assertNotNull($this->tokens->rotate($second->plainTextToken));
    }

    public function testReusingASpentTokenRevokesTheWholeFamily(): void
    {
        $first = $this->tokens->issue('user-1');
        $second = $this->tokens->rotate($first->plainTextToken);
        self::assertNotNull($second);
        $other = $this->tokens->issue('user-1');

        try {
            $this->tokens->rotate($first->plainTextToken);
            self::fail('Reuse must be detected.');
        } catch (RefreshTokenReusedException $reused) {
            self::assertSame($first->token->familyId, $reused->familyId);
            self::assertSame('user-1', $reused->subject);
        }
        self::assertNull($this->tokens->rotate($second->plainTextToken));
        self::assertNotNull($this->tokens->rotate($other->plainTextToken), 'Other sessions survive.');
    }

    public function testExpiredForgedAndRevokedTokensDoNotRotate(): void
    {
        $token = $this->tokens->issue('user-1');
        self::assertNull($this->tokens->rotate(substr($token->plainTextToken, 0, -1) . 'x'));
        self::assertNull($this->tokens->rotate('nbr_' . $token->token->id . '.' . str_repeat('a', 64)));

        $this->clock->now += 3_600;
        self::assertNull($this->tokens->rotate($token->plainTextToken));

        $live = $this->tokens->issue('user-1');
        self::assertTrue($this->tokens->revoke($live->plainTextToken));
        self::assertFalse($this->tokens->revoke('nbr_unknown'));
        self::assertNull($this->tokens->rotate($live->plainTextToken));

        $a = $this->tokens->issue('user-2');
        $b = $this->tokens->issue('user-2');
        self::assertSame(2, $this->tokens->revokeAll('user-2'));
        self::assertNull($this->tokens->rotate($a->plainTextToken));
        self::assertNull($this->tokens->rotate($b->plainTextToken));

        $this->clock->now += 3_600 + 86_400;
        self::assertSame(4, $this->tokens->prune());
    }

    public function testTokenPairsIssueAccessTokensAndRefreshThem(): void
    {
        $codec = new HmacTokenCodec(str_repeat('k', 32), 'https://auth.test', 'api', clock: $this->clock->closure());
        $pairs = new TokenPairIssuer($codec, $this->tokens, 600);

        $pair = $pairs->issue('user-9', ['orders:read']);
        $principal = $codec->verify($pair->accessToken);
        self::assertSame('user-9', $principal?->identifier());
        self::assertTrue($principal->can('orders:read'));
        self::assertSame(600, $pair->toArray()['expires_in']);
        self::assertSame('orders:read', $pair->toArray()['scope']);

        $next = $pairs->refresh($pair->refreshToken);
        self::assertSame('user-9', $codec->verify($next->accessToken)?->identifier());
        self::assertArrayHasKey('access_token', $pairs->issueLoginToken('user-9'));

        try {
            $pairs->refresh($pair->refreshToken);
            self::fail('A reused refresh token must fail.');
        } catch (HttpException $error) {
            self::assertSame(401, $error->status);
        }
        $this->expectException(HttpException::class);
        $pairs->refresh($next->refreshToken);
    }

    public function testTheDatabaseRevocationStoreIsShared(): void
    {
        $store = new DatabaseTokenRevocationStore($this->database->databases, clock: $this->clock->closure());
        $other = new DatabaseTokenRevocationStore($this->database->databases, clock: $this->clock->closure());
        $store->revoke('jti-1', $this->clock->now + 60);
        $store->revoke('jti-1', $this->clock->now + 120);

        self::assertTrue($other->isRevoked('jti-1', $this->clock->now));
        self::assertTrue($other->isRevoked('jti-1', $this->clock->now + 90));
        self::assertFalse($other->isRevoked('jti-2', $this->clock->now));
        self::assertFalse($other->isRevoked('jti-1', $this->clock->now + 120));
        $this->clock->now += 120;
        self::assertSame(1, $store->prune());
    }

    public function testRedisStoresAgainstARealServer(): void
    {
        $host = getenv('NIMBUS_TEST_REDIS');
        if (!is_string($host) || $host === '') {
            self::markTestSkipped('Set NIMBUS_TEST_REDIS=host:port to run against Redis.');
        }
        $redis = new SocketConnection($host);
        $revocations = new RedisTokenRevocationStore($redis, 'nimbus-test:revoked:');
        $revocations->revoke('jti-redis', time() + 60);
        self::assertTrue((new RedisTokenRevocationStore(new SocketConnection($host), 'nimbus-test:revoked:'))->isRevoked('jti-redis', time()));
        self::assertFalse($revocations->isRevoked('jti-other', time()));

        $states = new RedisOAuthStateStore($redis, 'nimbus-test:oauth:');
        $states->put('state-1', ['provider' => 'github', 'verifier' => 'v'], time() + 60);
        self::assertSame(['provider' => 'github', 'verifier' => 'v'], $states->pull('state-1', time()));
        self::assertNull($states->pull('state-1', time()));
        $redis->command(['DEL', 'nimbus-test:revoked:jti-redis']);
    }
}
