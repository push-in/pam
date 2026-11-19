<?php

declare(strict_types=1);

namespace Nimbus\Tests\Auth;

use GuzzleHttp\Client;
use GuzzleHttp\Handler\MockHandler;
use GuzzleHttp\HandlerStack;
use GuzzleHttp\Psr7\Response as GuzzleResponse;
use League\OAuth2\Client\Provider\GenericProvider;
use Nimbus\App;
use Nimbus\Auth\LoginTokenIssuer;
use Nimbus\Auth\OAuth\DatabaseOAuthStateStore;
use Nimbus\Auth\OAuth\MemoryOAuthStateStore;
use Nimbus\Auth\OAuth\OAuthStateStore;
use Nimbus\Auth\OAuth\SocialIdentity;
use Nimbus\Auth\OAuth\SocialLogin;
use Nimbus\Auth\OAuth\SocialUserResolver;
use Nimbus\Http\HttpException;
use Nimbus\Testing\TestClient;
use PHPUnit\Framework\Attributes\CoversClass;
use PHPUnit\Framework\TestCase;
use Psr\Http\Message\RequestInterface;

#[CoversClass(SocialLogin::class)]
#[CoversClass(SocialIdentity::class)]
#[CoversClass(MemoryOAuthStateStore::class)]
#[CoversClass(DatabaseOAuthStateStore::class)]
final class SocialLoginTest extends TestCase
{
    private MockHandler $http;

    /** @var list<RequestInterface> */
    private array $requests = [];

    /** @var list<SocialIdentity> */
    private array $identities = [];

    protected function setUp(): void
    {
        $this->http = new MockHandler();
        $this->requests = [];
        $this->identities = [];
    }

    public function testTheFlowUsesStatePkceAndTheAppResolver(): void
    {
        $login = $this->login(new MemoryOAuthStateStore());
        $start = $login->authorizationUrl('github', ['read:user', 'user:email']);
        parse_str((string) parse_url($start['authorization_url'], PHP_URL_QUERY), $query);

        self::assertSame($start['state'], $query['state'] ?? null);
        self::assertSame('S256', $query['code_challenge_method'] ?? null);
        self::assertSame('read:user,user:email', $query['scope'] ?? null);
        self::assertIsString($query['code_challenge'] ?? null);

        $this->exchange(['id' => 42, 'email' => 'ada@example.com', 'name' => 'Ada', 'email_verified' => true]);
        $tokens = $login->callback('github', $start['state'], 'the-code');

        self::assertSame(['subject' => 'user-42', 'abilities' => ['profile:read']], $tokens);
        self::assertSame('42', $this->identities[0]->id);
        self::assertSame('ada@example.com', $this->identities[0]->email);
        self::assertTrue($this->identities[0]->emailVerified);
        parse_str((string) $this->requests[0]->getBody(), $exchange);
        self::assertSame('the-code', $exchange['code'] ?? null);
        $verifier = $exchange['code_verifier'] ?? null;
        self::assertIsString($verifier);
        self::assertSame(
            $query['code_challenge'],
            rtrim(strtr(base64_encode(hash('sha256', $verifier, true)), '+/', '-_'), '='),
        );

        try {
            $login->callback('github', $start['state'], 'the-code');
            self::fail('A state works once.');
        } catch (HttpException $error) {
            self::assertSame(422, $error->status);
        }
    }

    public function testStatesAreBoundToTheirProviderAndProviderErrorsAreUnauthorized(): void
    {
        $login = $this->login(new MemoryOAuthStateStore());
        $start = $login->authorizationUrl('github');
        try {
            $login->callback('google', $start['state'], 'code');
            self::fail('Another provider cannot use the state.');
        } catch (HttpException $error) {
            self::assertSame(422, $error->status);
        }

        $start = $login->authorizationUrl('github');
        $this->http->append(new GuzzleResponse(400, ['content-type' => 'application/json'], '{"error":"invalid_grant"}'));
        try {
            $login->callback('github', $start['state'], 'stale');
            self::fail('Provider errors must be rejected.');
        } catch (HttpException $error) {
            self::assertSame(401, $error->status);
        }

        $this->expectException(HttpException::class);
        $login->authorizationUrl('unknown');
    }

    public function testRoutesWithTheDatabaseStateStore(): void
    {
        $database = AuthTestDatabase::create();
        try {
            $clock = new Clock(time());
            $states = new DatabaseOAuthStateStore($database->databases, clock: $clock->closure());
            $app = new App(discoverPackages: false);
            $this->login($states, $clock)->routes($app);
            $client = new TestClient($app);

            $start = $client->getJson('/auth/oauth/github')->assertOk();
            $state = $start->json('state');
            self::assertIsString($state);
            $this->exchange(['id' => 'abc']);
            $client->postJson('/auth/oauth/github/callback', ['state' => $state, 'code' => 'c'])
                ->assertOk()->assertJsonPath('subject', 'user-abc');
            $client->postJson('/auth/oauth/github/callback', ['state' => $state, 'code' => 'c'])->assertUnprocessableEntity();
            $client->postJson('/auth/oauth/github/callback', ['code' => 'c'])->assertJsonValidationErrors('state');
            $client->getJson('/auth/oauth/nope')->assertNotFound();

            $expired = $client->getJson('/auth/oauth/github')->json('state');
            self::assertIsString($expired);
            $clock->now += 601;
            $client->getJson('/auth/oauth/github/callback?state=' . $expired . '&code=c')->assertUnprocessableEntity();
            $client->getJson('/auth/oauth/github');
            $clock->now += 601;
            self::assertSame(1, $states->prune());
        } finally {
            $database->drop();
        }
    }

    /** @param array<string, mixed> $owner */
    private function exchange(array $owner): void
    {
        $this->http->append(
            new GuzzleResponse(200, ['content-type' => 'application/json'], '{"access_token":"provider-token","token_type":"bearer"}'),
            new GuzzleResponse(200, ['content-type' => 'application/json'], (string) json_encode($owner)),
        );
    }

    private function login(OAuthStateStore $states, ?Clock $clock = null): SocialLogin
    {
        $stack = HandlerStack::create($this->http);
        $stack->push(function (callable $handler): callable {
            return function (RequestInterface $request, array $options) use ($handler) {
                $this->requests[] = $request;
                return $handler($request, $options);
            };
        });
        $provider = new GenericProvider([
            'clientId' => 'client',
            'clientSecret' => 'secret',
            'redirectUri' => 'https://app.test/callback',
            'urlAuthorize' => 'https://provider.test/authorize',
            'urlAccessToken' => 'https://provider.test/token',
            'urlResourceOwnerDetails' => 'https://provider.test/user',
            'scopeSeparator' => ',',
        ], ['httpClient' => new Client(['handler' => $stack])]);
        $resolver = new class ($this->identities) implements SocialUserResolver {
            /** @param list<SocialIdentity> $identities */
            public function __construct(public array &$identities)
            {
            }

            public function resolve(SocialIdentity $identity): string
            {
                $this->identities[] = $identity;
                return 'user-' . $identity->id;
            }
        };
        $issuer = new class implements LoginTokenIssuer {
            public function issueLoginToken(string $subject, array $abilities = ['*']): array
            {
                return ['subject' => $subject, 'abilities' => $abilities];
            }
        };
        return new SocialLogin(
            ['github' => $provider, 'google' => static fn (): GenericProvider => clone $provider],
            $states,
            $resolver,
            $issuer,
            ['profile:read'],
            clock: $clock?->closure(),
        );
    }
}
