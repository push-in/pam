<?php

declare(strict_types=1);

namespace Nimbus\Tests\Auth;

use Nimbus\App;
use Nimbus\Auth\Abilities;
use Nimbus\Auth\Authenticator;
use Nimbus\Auth\ChainAuthenticator;
use Nimbus\Auth\PersonalAccessToken;
use Nimbus\Auth\PersonalAccessTokenAuthenticator;
use Nimbus\Auth\PersonalAccessTokens;
use Nimbus\Auth\Principal;
use Nimbus\Auth\TokenPrincipal;
use Nimbus\Testing\TestClient;
use Pam\Http\Request;
use Pam\Http\Response;
use PHPUnit\Framework\Attributes\CoversClass;
use PHPUnit\Framework\TestCase;

#[CoversClass(PersonalAccessTokens::class)]
#[CoversClass(PersonalAccessToken::class)]
#[CoversClass(PersonalAccessTokenAuthenticator::class)]
#[CoversClass(ChainAuthenticator::class)]
#[CoversClass(Abilities::class)]
final class PersonalAccessTokensTest extends TestCase
{
    private AuthTestDatabase $database;

    private Clock $clock;

    private PersonalAccessTokens $tokens;

    protected function setUp(): void
    {
        $this->database = AuthTestDatabase::create();
        $this->clock = new Clock();
        $this->tokens = new PersonalAccessTokens($this->database->databases, clock: $this->clock->closure());
    }

    protected function tearDown(): void
    {
        $this->database->drop();
    }

    public function testThePlainTokenIsShownOnceAndOnlyItsHashIsStored(): void
    {
        $created = $this->tokens->create('user-1', 'CLI', ['products:read', 'products:write', 'products:read']);

        self::assertMatchesRegularExpression('/^nbp_[a-f0-9]{24}\.[a-f0-9]{64}$/', $created->plainTextToken);
        self::assertSame(['products:read', 'products:write'], $created->accessToken->abilities);
        $row = $this->database->databases->database()->select(['token_hash'])->from('personal_access_tokens')->fetchAll()[0] ?? [];
        self::assertIsArray($row);
        $secret = explode('.', $created->plainTextToken)[1];
        self::assertSame(hash('sha256', $secret), $row['token_hash']);
        self::assertStringNotContainsString($secret, (string) json_encode($row));

        $found = $this->tokens->find($created->plainTextToken);
        self::assertNotNull($found);
        self::assertSame('user-1', $found->subject);
        self::assertTrue($found->can('products:write'));
        self::assertFalse($found->can('orders:read'));
        self::assertSame('CLI', $created->toArray()['name']);
    }

    public function testForgedMalformedAndExpiredTokensAreRejected(): void
    {
        $created = $this->tokens->create('user-1', 'CI', ['*'], $this->clock->now + 60);
        [$id] = explode('.', substr($created->plainTextToken, 4));

        self::assertNull($this->tokens->find('nbp_' . $id . '.' . str_repeat('0', 64)));
        self::assertNull($this->tokens->find(substr($created->plainTextToken, 4)));
        self::assertNull($this->tokens->find('garbage'));
        self::assertNotNull($this->tokens->find($created->plainTextToken));

        $this->clock->now += 60;
        self::assertNull($this->tokens->find($created->plainTextToken));
        self::assertSame(1, $this->tokens->prune());
    }

    public function testUseRecordsLastUsedAtAtTheConfiguredResolution(): void
    {
        $created = $this->tokens->create('user-1', 'Phone');

        self::assertSame($this->clock->now, $this->lastUsed($created->plainTextToken));
        $first = $this->clock->now;
        $this->clock->now += 30;
        self::assertSame($first, $this->lastUsed($created->plainTextToken));
        $this->clock->now += 30;
        self::assertSame($this->clock->now, $this->lastUsed($created->plainTextToken));
    }

    private function lastUsed(string $token): ?int
    {
        $used = $this->tokens->use($token);
        self::assertNotNull($used);
        return $used->lastUsedAt;
    }

    public function testRevokeOneOrAllTokensOfASubject(): void
    {
        $one = $this->tokens->create('user-1', 'A');
        $two = $this->tokens->create('user-1', 'B');
        $other = $this->tokens->create('user-2', 'C');

        self::assertCount(2, $this->tokens->tokensFor('user-1'));
        self::assertFalse($this->tokens->revoke($one->accessToken->id, 'user-2'));
        self::assertTrue($this->tokens->revoke($one->accessToken->id, 'user-1'));
        self::assertNull($this->tokens->find($one->plainTextToken));
        self::assertNotNull($this->tokens->find($two->plainTextToken));

        self::assertSame(1, $this->tokens->revokeAll('user-1'));
        self::assertNull($this->tokens->find($two->plainTextToken));
        self::assertNotNull($this->tokens->find($other->plainTextToken));
    }

    public function testRoutesCheckTokenAbilitiesThroughCan(): void
    {
        $read = $this->tokens->create('user-1', 'read', ['products:read'])->plainTextToken;
        $write = $this->tokens->create('user-1', 'write', ['products:*'])->plainTextToken;
        $app = new App(discoverPackages: false);
        $app->container()->instance(Authenticator::class, new ChainAuthenticator(
            new class implements Authenticator {
                public function authenticate(Request $request): ?Principal
                {
                    return null;
                }
            },
            new PersonalAccessTokenAuthenticator($this->tokens),
        ));
        $app->post('/products', static fn (Request $request, Response $response, Principal $principal): Response => $response
            ->json(['by' => $principal->identifier()], 201))
            ->can('products:write');
        $client = new TestClient($app);

        $client->postJson('/products')->assertUnauthorized();
        $client->withToken('nbp_nope')->postJson('/products')->assertUnauthorized();
        (new TestClient($app))->withToken($read)->postJson('/products')->assertForbidden();
        (new TestClient($app))->withToken($write)->postJson('/products')->assertCreated()->assertJsonPath('by', 'user-1');
    }

    public function testAbilityWildcards(): void
    {
        self::assertTrue(Abilities::allows(['*'], 'anything'));
        self::assertTrue(Abilities::allows(['orders.*'], 'orders.read'));
        self::assertFalse(Abilities::allows(['orders.*'], 'ordersx.read'));
        self::assertFalse(Abilities::allows(['orders:read'], 'orders:write'));
        self::assertFalse(Abilities::allows(['orders:*'], ''));
        self::assertTrue((new TokenPrincipal('u', ['products:*'], 'jti'))->can('products:delete'));
        $this->expectException(\InvalidArgumentException::class);
        Abilities::normalize(['has space']);
    }
}
