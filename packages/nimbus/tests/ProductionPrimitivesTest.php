<?php

declare(strict_types=1);

namespace Nimbus\Tests;

use Nimbus\App;
use Nimbus\Container\Container;
use Nimbus\Events\SyncEventDispatcher;
use Nimbus\Health\HealthCheck;
use Nimbus\Health\HealthRegistry;
use Nimbus\Health\HealthResult;
use Nimbus\Health\HealthState;
use Nimbus\Jobs\JobState;
use Nimbus\Jobs\MemoryJobDispatcher;
use Nimbus\Middleware\CorsMiddleware;
use Nimbus\Middleware\ObserveRequestMiddleware;
use Nimbus\Observability\RequestObservation;
use Nimbus\Observability\RequestObserver;
use Nimbus\Resilience\CircuitBreaker;
use Nimbus\Resilience\CircuitState;
use Nimbus\Resilience\RetryPolicy;
use Nimbus\Validation\DtoHydrator;
use Pam\Http\Request;
use Pam\Http\Response;
use PHPUnit\Framework\TestCase;

final class ProductionPrimitivesTest extends TestCase
{
    public function testDtoHydrationSupportsIntegerBackedEnums(): void
    {
        $dto = (new DtoHydrator())->hydrate(CreateAccountData::class, [
            'email' => 'dev@pam.dev',
            'type' => 2,
        ]);

        self::assertInstanceOf(CreateAccountData::class, $dto);
        self::assertSame(AccountType::Administrator, $dto->type);
    }

    public function testEventsJobsRetryAndCircuitStatesAreDeterministic(): void
    {
        $events = new SyncEventDispatcher(new Container());
        $received = null;
        $events->listen(AccountCreated::class, static function (AccountCreated $event) use (&$received): void {
            $received = $event->id;
        });
        $events->dispatch(new AccountCreated(10));
        self::assertSame(10, $received);

        $job = (new MemoryJobDispatcher())->dispatch(new SendWelcomeEmail(10));
        self::assertSame(JobState::Pending, $job->state);

        $attempts = 0;
        $result = (new RetryPolicy(attempts: 3, initialDelayMilliseconds: 0))->run(
            static function () use (&$attempts): string {
                if (++$attempts < 3) {
                    throw new \RuntimeException('temporary');
                }
                return 'ok';
            },
        );
        self::assertSame('ok', $result);
        self::assertSame(3, $attempts);

        $circuit = new CircuitBreaker(failureThreshold: 1);
        try {
            $circuit->call(static fn (): never => throw new \RuntimeException('down'));
        } catch (\RuntimeException) {
        }
        self::assertSame(CircuitState::Open, $circuit->state());
    }

    public function testHealthRegistryReturnsTheWorstIntegerState(): void
    {
        $registry = new HealthRegistry();
        $registry->add('database', new class implements HealthCheck {
            public function check(): HealthResult
            {
                return new HealthResult(HealthState::Degraded, ['latencyMs' => 150]);
            }
        });

        self::assertSame(HealthState::Degraded->value, $registry->run()['state']);
    }

    public function testCorsPreflightValidatesRequestedMethod(): void
    {
        $app = new App(discoverPackages: false);
        $app->middleware(new CorsMiddleware(['https://app.example.com'], methods: ['GET']));
        $app->route('OPTIONS', '/resource', static fn (Request $request, Response $response): Response => $response->send(null));

        $denied = $app->handle($this->preflight('DELETE'), new Response())->export();
        $allowed = $app->handle($this->preflight('GET'), new Response())->export();

        self::assertSame(403, $denied['status']);
        self::assertSame(204, $allowed['status']);
    }

    public function testCorsRefusesAnyOriginWithCredentials(): void
    {
        // Reflecting every Origin with credentials lets any site read authenticated responses.
        $this->expectException(\InvalidArgumentException::class);
        new CorsMiddleware(['*'], credentials: true);
    }

    public function testObservationsUseNormalizedRouteTemplates(): void
    {
        $observer = new CollectingObserver();
        $app = new App(discoverPackages: false);
        $app->middleware(new ObserveRequestMiddleware($observer, $app->container()));
        $app->get('/users/{id}', static fn (Request $request, Response $response): Response => $response->send('ok'));

        $app->handle(new Request('GET', '/users/42', [], [], ''), new Response());

        self::assertNotNull($observer->last);
        self::assertSame('/users/{id}', $observer->last->route);
        self::assertSame(200, $observer->last->status);
    }

    private function preflight(string $method): Request
    {
        return new Request('OPTIONS', '/resource', [], [
            'origin' => ['https://app.example.com'],
            'access-control-request-method' => [$method],
        ], '');
    }
}

enum AccountType: int
{
    case Regular = 1;
    case Administrator = 2;
}

final readonly class CreateAccountData
{
    public function __construct(public string $email, public AccountType $type)
    {
    }
}

final readonly class AccountCreated
{
    public function __construct(public int $id)
    {
    }
}

final readonly class SendWelcomeEmail
{
    public function __construct(public int $accountId)
    {
    }
}

final class CollectingObserver implements RequestObserver
{
    public ?RequestObservation $last = null;

    public function record(RequestObservation $observation): void
    {
        $this->last = $observation;
    }
}
