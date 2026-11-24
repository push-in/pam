<?php

declare(strict_types=1);

namespace Nimbus\Tests\Http;

use Nimbus\App;
use Nimbus\Cache\Cache;
use Nimbus\Container\Container;
use Nimbus\Http\HttpException;
use Nimbus\Http\JsonEncoding;
use Nimbus\Http\JsonResource;
use Nimbus\Http\LaravelErrorRenderer;
use Nimbus\Http\ProblemCode;
use Nimbus\Http\ProblemDetailsRenderer;
use Nimbus\Http\RequestContext;
use Nimbus\Middleware\ThrottleRequestsMiddleware;
use Nimbus\RateLimit\Limit;
use Nimbus\RateLimit\RateLimiter;
use Nimbus\Routing\RouteBindable;
use Nimbus\Testing\TestClient;
use Nimbus\Validation\ValidationException;
use Pam\Contracts\Http\MiddlewareInterface;
use Pam\Contracts\Http\RequestHandlerInterface;
use Pam\Http\Request;
use Pam\Http\Response;
use PHPUnit\Framework\Attributes\CoversClass;
use PHPUnit\Framework\TestCase;

#[CoversClass(JsonEncoding::class)]
#[CoversClass(LaravelErrorRenderer::class)]
#[CoversClass(ProblemDetailsRenderer::class)]
#[CoversClass(RequestContext::class)]
#[CoversClass(RateLimiter::class)]
#[CoversClass(Limit::class)]
#[CoversClass(ThrottleRequestsMiddleware::class)]
final class LaravelParityTest extends TestCase
{
    protected function tearDown(): void
    {
        JsonEncoding::use(JsonEncoding::DEFAULT);
        Cache::useRepository(null);
    }

    public function testJsonEncodingDefaultsToUnescapedUnicodeAndSwitchesToLaravelBytes(): void
    {
        $app = new App(discoverPackages: false);
        $app->get('/city', static fn (): array => ['city' => 'São Paulo', 'url' => 'https://pam.dev/a']);
        $app->get('/resource', static fn (): JsonResource => new ParityCityResource('São Paulo'));
        $client = new TestClient($app);

        self::assertSame('{"city":"São Paulo","url":"https:\/\/pam.dev\/a"}', $client->get('/city')->body());
        self::assertSame('application/json; charset=utf-8', $client->get('/city')->header('content-type'));

        JsonEncoding::use(JsonEncoding::LARAVEL);
        self::assertSame('{"city":"S\\u00e3o Paulo","url":"https:\/\/pam.dev\/a"}', $client->get('/city')->body());
        self::assertSame('{"data":{"city":"S\\u00e3o Paulo"}}', $client->get('/resource')->body());

        $app2 = new App(discoverPackages: false);
        $app2->jsonEncoding(JSON_UNESCAPED_UNICODE | JSON_UNESCAPED_SLASHES);
        self::assertSame(JSON_UNESCAPED_UNICODE | JSON_UNESCAPED_SLASHES, JsonEncoding::flags());
        self::assertSame('{"a":"/b"}', JsonEncoding::encode(['a' => '/b']));
    }

    public function testActionsReturningArraysKeepTheStatusTheyChose(): void
    {
        $app = new App(discoverPackages: false);
        $app->post('/items', static function (Response $response): array {
            $response->status(201);
            return ['id' => 1];
        });

        (new TestClient($app))->postJson('/items')->assertStatus(201)->assertJsonPath('id', 1);
        self::addToAssertionCount(2);
    }

    public function testTheLaravelRendererWritesMessageBodiesForEveryError(): void
    {
        $app = new App(discoverPackages: false);
        $app->renderErrorsWith(new LaravelErrorRenderer());
        $app->get('/invalid', static function (): never {
            throw new ValidationException([
                'name' => ['The name field is required.'],
                'email' => ['The email field must be a valid email address.', 'Other.'],
            ]);
        });
        $app->get('/conflict', static function (): never {
            throw new HttpException(409, ProblemCode::Conflict, 'Version conflict.');
        });
        $app->get('/explode', static function (): never {
            throw new \RuntimeException('secret');
        });
        $client = new TestClient($app);

        $invalid = $client->get('/invalid')->assertStatus(422);
        self::assertSame('application/json; charset=utf-8', $invalid->header('content-type'));
        self::assertSame([
            'message' => 'The name field is required. (and 2 more errors)',
            'errors' => [
                'name' => ['The name field is required.'],
                'email' => ['The email field must be a valid email address.', 'Other.'],
            ],
        ], $invalid->json());
        self::assertSame(['message' => 'Version conflict.'], $client->get('/conflict')->assertStatus(409)->json());
        self::assertSame(['message' => 'Not Found'], $client->get('/missing')->assertStatus(404)->json());
        self::assertSame(['message' => 'Method Not Allowed'], $client->postJson('/conflict')->assertStatus(405)->json());
        $error = $client->get('/explode')->assertStatus(500);
        self::assertSame(['message' => 'Server Error'], $error->json());
    }

    public function testProblemDetailsStayTheDefaultAndTheRendererIsInTheContainer(): void
    {
        $app = new App(discoverPackages: false);
        self::assertInstanceOf(ProblemDetailsRenderer::class, $app->container()->get(\Nimbus\Http\ErrorRenderer::class));
        $client = new TestClient($app);
        $client->get('/missing')
            ->assertStatus(404)
            ->assertHeader('content-type', 'application/problem+json; charset=utf-8')
            ->assertJsonPath('code', ProblemCode::NotFound->value);
        self::addToAssertionCount(3);
    }

    public function testNamedLimitersCountPerKeyAndSendLaravelHeaders(): void
    {
        $app = new App(discoverPackages: false);
        $app->renderErrorsWith(new LaravelErrorRenderer());
        $app->rateLimiter()->for('buck-ask', static fn (Request $request): Limit => Limit::perMinute(2)->by(
            $request->getHeader('x-account') ?? 'anonymous',
        ));
        $app->get('/ask', static fn (): array => ['ok' => true])->rateLimit('buck-ask');
        $client = new TestClient($app);

        $first = $client->get('/ask', ['x-account' => 'a'])->assertOk();
        self::assertSame('2', $first->header('x-ratelimit-limit'));
        self::assertSame('1', $first->header('x-ratelimit-remaining'));
        $client->get('/ask', ['x-account' => 'a'])->assertOk()->assertHeader('x-ratelimit-remaining', '0');
        $limited = $client->get('/ask', ['x-account' => 'a'])->assertStatus(429);
        self::assertSame(['message' => 'Too Many Attempts.'], $limited->json());
        $retryAfter = (int) $limited->header('retry-after');
        self::assertGreaterThanOrEqual(1, $retryAfter);
        self::assertLessThanOrEqual(60, $retryAfter);
        self::assertGreaterThanOrEqual(time(), (int) $limited->header('x-ratelimit-reset'));

        $client->get('/ask', ['x-account' => 'b'])->assertOk();
    }

    public function testInlineThrottleExpressesLimitsBelowOnePerSecond(): void
    {
        $app = new App(discoverPackages: false);
        $app->post('/login', static fn (): array => ['ok' => true])->throttle(3, 1);
        $app->post('/alias', static fn (): array => ['ok' => true])->middleware('throttle:1,1');
        $client = new TestClient($app);

        for ($attempt = 0; $attempt < 3; ++$attempt) {
            $client->postJson('/login')->assertOk();
        }
        $client->postJson('/login')->assertStatus(429)->assertJsonPath('code', ProblemCode::RateLimited->value);
        // Each route counts on its own.
        $client->postJson('/alias')->assertOk();
        $client->postJson('/alias')->assertStatus(429);
        self::addToAssertionCount(6);
    }

    public function testLimitsCanBeUnlimitedOrAnswerTheirOwnResponse(): void
    {
        $app = new App(discoverPackages: false);
        $limiter = $app->rateLimiter();
        $limiter->for('free', static fn (): Limit => Limit::none());
        $limiter->for('custom', static fn (): array => [
            Limit::perHour(1)->response(static fn (Request $request, Response $response, array $headers): Response => $response
                ->status(429)
                ->send('slow down, retry in ' . $headers['retry-after'])),
        ]);
        $app->get('/free', static fn (): array => [])->rateLimit('free');
        $app->get('/custom', static fn (): array => [])->rateLimit('custom');
        $client = new TestClient($app);

        $client->get('/free')->assertOk();
        $client->get('/free')->assertOk()->assertHeaderMissing('x-ratelimit-limit');
        $client->get('/custom')->assertOk();
        self::assertStringStartsWith('slow down, retry in ', $client->get('/custom')->assertStatus(429)->body());
    }

    public function testTheRateLimiterApiMatchesLaravel(): void
    {
        $limiter = new RateLimiter(new \Nimbus\Cache\Repository(new \Nimbus\Cache\MemoryStore()));

        self::assertSame('ran', $limiter->attempt('send:1', 2, static fn (): string => 'ran'));
        self::assertTrue($limiter->attempt('send:1', 2, static fn (): null => null));
        self::assertFalse($limiter->attempt('send:1', 2, static fn (): string => 'ran'));
        self::assertTrue($limiter->tooManyAttempts('send:1', 2));
        self::assertSame(0, $limiter->remaining('send:1', 2));
        self::assertGreaterThan(0, $limiter->availableIn('send:1'));
        $limiter->clear('send:1');
        self::assertSame(2, $limiter->remaining('send:1', 2));
        self::assertSame(1, $limiter->hit('send:1', 30));

        $app = new App(discoverPackages: false);
        $app->get('/x', static fn (): array => [])->rateLimit('undefined');
        (new TestClient($app))->get('/x')->assertStatus(500);
    }

    public function testMiddlewarePassesTypedAndKeyedValuesToTheAction(): void
    {
        $app = new App(discoverPackages: false);
        $app->middleware(new ParityAccountMiddleware($app->container()));
        $app->get('/accounts/{account}', static fn (ParityAccount $account, RequestContext $context): array => [
            'account' => $account->id,
            'source' => $account->source,
            'monitor' => $context->get('monitor'),
            'typed' => $context->typed(ParityAccount::class)?->id,
        ]);
        $app->get('/plain/{account}', static fn (ParityAccount $account): array => ['source' => $account->source]);

        self::assertSame(
            ['account' => 'acme', 'source' => 'middleware', 'monitor' => 'm-1', 'typed' => 'acme'],
            (new TestClient($app))->get('/accounts/acme', ['x-account' => 'acme'])->assertOk()->json(),
        );
        // Without a context value the route segment still goes through route binding.
        self::assertSame(['source' => 'route'], (new TestClient($app))->get('/plain/acme')->assertOk()->json());
    }

    public function testTheContextIsScopedToOneRequest(): void
    {
        $app = new App(discoverPackages: false);
        $app->get('/peek', static fn (RequestContext $context): array => ['seen' => $context->get('monitor', 'none')]);
        $app->get('/set', static function (RequestContext $context): array {
            $context->set('monitor', 'set');
            return ['all' => $context->all(), 'has' => $context->has('monitor')];
        });
        $client = new TestClient($app);

        self::assertSame(['all' => ['monitor' => 'set'], 'has' => true], $client->get('/set')->json());
        self::assertSame(['seen' => 'none'], $client->get('/peek')->json());
    }
}

final readonly class ParityCityResource extends JsonResource
{
    public function toArray(Request $request): array
    {
        return ['city' => $this->resource];
    }
}

final class ParityAccount implements RouteBindable
{
    public function __construct(public string $id, public string $source)
    {
    }

    public static function resolveRouteBinding(string $value): static
    {
        return new static($value, 'route');
    }
}

final readonly class ParityAccountMiddleware implements MiddlewareInterface
{
    public function __construct(private Container $container)
    {
    }

    public function process(Request $request, Response $response, RequestHandlerInterface $next): Response
    {
        $account = $request->getHeader('x-account');
        if ($account !== null) {
            $context = $this->container->get(RequestContext::class);
            self::assertContext($context);
            $context->put(new ParityAccount($account, 'middleware'));
            $context->set('monitor', 'm-1');
        }
        return $next->handle($request, $response);
    }

    /** @phpstan-assert RequestContext $context */
    private static function assertContext(mixed $context): void
    {
        if (!$context instanceof RequestContext) {
            throw new \LogicException('RequestContext is not bound.');
        }
    }
}
