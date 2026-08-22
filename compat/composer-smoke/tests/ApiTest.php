<?php

declare(strict_types=1);

namespace Pam\Compatibility\Tests;

use Pam\App;
use Pam\Contracts\Http\ApplicationInterface;
use Pam\Contracts\Package\ServiceProviderInterface;
use Pam\Contracts\Transport\MessageDisposition;
use Pam\Contracts\Transport\MessageResult;
use Pam\Contracts\Transport\TransportCapability;
use Pam\Contracts\Transport\TransportContext;
use Pam\Contracts\Transport\TransportDescriptor;
use Pam\Contracts\Transport\TransportKind;
use Pam\Contracts\Transport\TransportMessage;
use Pam\Contracts\Transport\TransportProviderInterface;
use Pam\Contracts\Transport\TransportWorker;
use Pam\Api\Middleware\SecurityHeadersMiddleware;
use Pam\Api\Middleware\RateLimitMiddleware;
use Pam\Api\PackageDiscovery;
use Pam\Api\Router;
use Pam\Api\RoutingResultType;
use Pam\Http\Request;
use Pam\Http\Response;
use Pam\Testing\TestClient;
use PHPUnit\Framework\TestCase;

final class FixtureServiceProvider implements ServiceProviderInterface
{
    public function register(ApplicationInterface $application): void
    {
    }

    public function boot(ApplicationInterface $application): void
    {
    }
}

final class FixtureTransport implements TransportProviderInterface
{
    public bool $started = false;
    public bool $stopped = false;
    /** @var list<MessageDisposition> */
    public array $dispositions = [];

    public function __construct(
        private readonly bool $failOnStart = false,
        private readonly bool $supportsDelayedRetry = true,
    ) {
    }

    public function descriptor(): TransportDescriptor
    {
        return new TransportDescriptor(
            id: 'fixture.queue',
            kind: TransportKind::Queue,
            capabilities: $this->supportsDelayedRetry
                ? [
                    TransportCapability::Publish,
                    TransportCapability::Consume,
                    TransportCapability::DelayedRetry,
                ]
                : [TransportCapability::Publish, TransportCapability::Consume],
            maxPayloadBytes: 4_096,
            maxBatchSize: 10,
        );
    }

    public function start(TransportContext $context): void
    {
        $this->started = !$context->isCancelled();
        if ($this->failOnStart) {
            throw new \RuntimeException('start failed');
        }
    }

    public function receive(int $maximum, int $waitMilliseconds): iterable
    {
        yield new TransportMessage('message-1', 'orders.created', '{"id":42}');
    }

    public function publish(string $topic, string $payload, array $headers = []): void
    {
    }

    public function acknowledge(
        TransportMessage $message,
        MessageDisposition $disposition,
        ?int $retryAfterMilliseconds = null,
    ): void {
        $this->dispositions[] = $disposition;
    }

    public function stop(): void
    {
        $this->stopped = true;
    }
}

final class ApiTest extends TestCase
{
    public function testRegistersVersionedBoundedNonHttpTransports(): void
    {
        $app = new App(discoverPackages: false);
        $transport = new FixtureTransport();
        self::assertSame($app, $app->transport($transport));
        self::assertSame(['fixture.queue' => $transport], $app->transports());

        $events = [];
        $context = new TransportContext(
            'worker-1',
            static fn (): bool => false,
            static function (int $eventCode, array $attributes) use (&$events): void {
                $events[] = [$eventCode, $attributes];
            },
        );
        $handled = [];
        $processed = TransportWorker::run(
            $transport,
            static function (TransportMessage $message) use (&$handled): MessageResult {
                $handled[] = $message;
                return MessageResult::acknowledge();
            },
            $context,
            maximumMessages: 1,
            waitMilliseconds: 100,
        );
        self::assertTrue($transport->started);
        self::assertSame(1, $processed);
        self::assertSame('orders.created', $handled[0]->topic);
        self::assertLessThanOrEqual(
            $transport->descriptor()->maxPayloadBytes,
            strlen($handled[0]->payload),
        );
        self::assertSame([MessageDisposition::Acknowledge], $transport->dispositions);
        self::assertTrue($transport->stopped);
        self::assertSame([1, 2, 3, 4, 9], array_column($events, 0));

        $failing = new FixtureTransport();
        TransportWorker::run(
            $failing,
            static fn (TransportMessage $message): MessageResult => throw new \RuntimeException('fixture'),
            new TransportContext(
                'worker-2',
                static fn (): bool => false,
                static function (int $eventCode, array $attributes): void {},
            ),
            maximumMessages: 1,
        );
        self::assertSame([MessageDisposition::Retry], $failing->dispositions);

        $this->expectException(\LogicException::class);
        $app->transport(new FixtureTransport());
    }

    public function testTransportContractsRejectUnsafeOrUnboundedMetadata(): void
    {
        $this->expectException(\InvalidArgumentException::class);
        new TransportDescriptor(
            id: '../escape',
            kind: TransportKind::Queue,
            capabilities: [TransportCapability::Consume],
        );
    }

    public function testTransportContractsValidateUntypedRuntimeInput(): void
    {
        try {
            new TransportDescriptor(
                id: 'fixture.invalid',
                kind: TransportKind::Queue,
                capabilities: [1],
            );
            self::fail('Integer capabilities must be rejected.');
        } catch (\InvalidArgumentException) {
        }

        $this->expectException(\InvalidArgumentException::class);
        new TransportMessage(
            id: 'message-1',
            topic: 'orders.created',
            payload: '{}',
            headers: ['trace-id' => 42],
        );
    }

    public function testTransportWorkerAlwaysStopsAfterStartupFailure(): void
    {
        $transport = new FixtureTransport(failOnStart: true);
        $events = [];

        try {
            TransportWorker::run(
                $transport,
                static fn (TransportMessage $message): MessageResult => MessageResult::acknowledge(),
                new TransportContext(
                    'worker-failing-start',
                    static fn (): bool => false,
                    static function (int $eventCode, array $attributes) use (&$events): void {
                        $events[] = $eventCode;
                    },
                ),
                maximumMessages: 1,
            );
            self::fail('Startup failure must propagate.');
        } catch (\RuntimeException $error) {
            self::assertSame('start failed', $error->getMessage());
        }

        self::assertTrue($transport->stopped);
        self::assertSame([1, 9], $events);
    }

    public function testTransportWorkerRejectsUnsupportedRetryDisposition(): void
    {
        $transport = new FixtureTransport(supportsDelayedRetry: false);

        $this->expectException(\LogicException::class);
        $this->expectExceptionMessage('does not support delayed retries');
        try {
            TransportWorker::run(
                $transport,
                static fn (TransportMessage $message): MessageResult => MessageResult::retry(100),
                new TransportContext(
                    'worker-no-retry',
                    static fn (): bool => false,
                    static function (int $eventCode, array $attributes): void {},
                ),
                maximumMessages: 1,
            );
        } finally {
            self::assertTrue($transport->stopped);
        }
    }

    public function testRouterMatchesParametersAndReportsAllowedMethods(): void
    {
        $router = new Router();
        $router->add('GET', '/users/{id}', static fn (): string => 'user');
        $router->add('GET', '/users/me', static fn (): string => 'current-user');

        $match = $router->match('GET', '/users/42');
        self::assertSame(RoutingResultType::Found, $match->type);
        self::assertSame(['id' => '42'], $match->parameters);

        $methodMismatch = $router->match('POST', '/users/42');
        self::assertSame(RoutingResultType::MethodNotAllowed, $methodMismatch->type);
        self::assertSame(['GET', 'HEAD', 'OPTIONS'], $methodMismatch->allowedMethods);

        self::assertSame(RoutingResultType::NotFound, $router->match('GET', '/missing')->type);
        self::assertSame('/users/me', $router->match('GET', '/users/me')->route?->path);
    }

    public function testApiPipelineRunsInsidePam(): void
    {
        if (!class_exists(Request::class, false)) {
            self::markTestSkipped('This contract executes inside the Pam Embed SAPI.');
        }

        $app = new App(discoverPackages: false);
        $app->middleware(new SecurityHeadersMiddleware(hsts: false));
        $app->get('/users/{id}', static fn (Request $request, Response $response): Response =>
            $response->json(['id' => $request->route('id')]));

        $response = $app->handle(
            new Request('GET', '/users/42', [], [], ''),
            new Response(),
        )->export();

        self::assertSame(200, $response['status']);
        self::assertSame('{"id":"42"}', $response['body']);
        self::assertSame(['nosniff'], $response['headers']['x-content-type-options']);
    }

    public function testInMemoryClientExercisesTheApiWithoutOpeningAPort(): void
    {
        if (!class_exists(Request::class, false)) {
            self::markTestSkipped('This contract executes inside the Pam Embed SAPI.');
        }

        $app = new App(discoverPackages: false);
        $app->get('/users/{id}', static fn (Request $request, Response $response): Response =>
            $response->json([
                'id' => $request->route('id'),
                'expand' => $request->getQuery('expand'),
            ]));

        $testResponse = (new TestClient($app))
            ->get('/users/42?expand=profile')
            ->assertSuccessful()
            ->assertHeader('content-type', 'application/json; charset=utf-8')
            ->assertJsonPath('id', '42')
            ->assertJson([
                'id' => '42',
                'expand' => 'profile',
            ]);
        self::assertSame(['id' => '42', 'expand' => 'profile'], $testResponse->json());
    }

    public function testComposerProviderDiscoveryBuildsAnAtomicCache(): void
    {
        $root = sys_get_temp_dir() . '/pam-package-discovery-' . bin2hex(random_bytes(8));
        $composerDirectory = $root . '/vendor/composer';
        self::assertTrue(mkdir($composerDirectory, 0755, true));

        try {
            file_put_contents($composerDirectory . '/installed.json', json_encode([
                'packages' => [[
                    'name' => 'acme/pam-fixture',
                    'extra' => [
                        'pam' => [
                            'providers' => [FixtureServiceProvider::class],
                        ],
                    ],
                ]],
            ], JSON_THROW_ON_ERROR));

            self::assertSame([FixtureServiceProvider::class], PackageDiscovery::providers($root));
            self::assertFileExists($root . '/.pam/cache/packages.json');
            self::assertSame([FixtureServiceProvider::class], PackageDiscovery::providers($root));

            file_put_contents($root . '/.pam/cache/packages.json', '<?php echo "unsafe";');
            touch($root . '/.pam/cache/packages.json', time() + 5);
            $this->expectException(\JsonException::class);
            PackageDiscovery::providers($root);
        } finally {
            self::removeDirectory($root);
        }
    }

    public function testRateLimitStateIsBoundedAndHeadersRejectInjection(): void
    {
        if (!class_exists(Request::class, false)) {
            self::markTestSkipped('This contract executes inside the Pam Embed SAPI.');
        }

        $server = $_SERVER;
        try {
            $app = new App(discoverPackages: false);
            $app->middleware(new RateLimitMiddleware(100, maxBuckets: 1));
            $app->get('/ping', static fn (Request $request, Response $response): Response =>
                $response->json(['message' => 'pong']));
            $client = new TestClient($app);

            $_SERVER['REMOTE_ADDR'] = '192.0.2.1';
            self::assertSame(200, $client->get('/ping')->status);
            $_SERVER['REMOTE_ADDR'] = '192.0.2.2';
            self::assertSame(429, $client->get('/ping')->status);

            $this->expectException(\InvalidArgumentException::class);
            (new Response())->header('x-safe', "value\r\nx-injected: true");
        } finally {
            $_SERVER = $server;
        }
    }

    private static function removeDirectory(string $directory): void
    {
        if (!is_dir($directory)) {
            return;
        }
        $iterator = new \RecursiveIteratorIterator(
            new \RecursiveDirectoryIterator($directory, \FilesystemIterator::SKIP_DOTS),
            \RecursiveIteratorIterator::CHILD_FIRST,
        );
        foreach ($iterator as $entry) {
            if (!$entry instanceof \SplFileInfo) {
                continue;
            }
            if ($entry->isDir()) {
                rmdir($entry->getPathname());
            } else {
                unlink($entry->getPathname());
            }
        }
        rmdir($directory);
    }
}
