<?php

declare(strict_types=1);

namespace Nimbus\Tests\Sentry;

use Nimbus\App;
use Nimbus\Http\HttpException;
use Nimbus\Http\ProblemCode;
use Nimbus\Jobs\JobOutcome;
use Nimbus\Sentry\SentryJobHandler;
use Nimbus\Sentry\SentryReporter;
use Pam\Http\Request;
use Pam\Http\Response;
use PHPUnit\Framework\Attributes\CoversClass;
use PHPUnit\Framework\TestCase;
use Sentry\ClientBuilder;
use Sentry\Event;
use Sentry\SentrySdk;
use Sentry\State\Scope;
use Sentry\Transport\Result;
use Sentry\Transport\ResultStatus;
use Sentry\Transport\TransportInterface;

#[CoversClass(SentryReporter::class)]
#[CoversClass(SentryJobHandler::class)]
final class SentryReporterTest extends TestCase
{
    private RecordingTransport $transport;

    protected function setUp(): void
    {
        $this->transport = new RecordingTransport();
        $client = ClientBuilder::create(['dsn' => 'https://public@sentry.example/1', 'default_integrations' => false])
            ->setTransport($this->transport)
            ->getClient();
        SentrySdk::init()->bindClient($client);
        \Sentry\configureScope(static function (Scope $scope): void {
            $scope->setTag('service', 'orders');
        });
    }

    protected function tearDown(): void
    {
        SentrySdk::init();
    }

    public function testReportsServerErrorsWithRouteAndNotClientErrors(): void
    {
        $app = new App(discoverPackages: false);
        (new SentryReporter($app->container()))->install($app);
        $app->get('/orders/{id}', static function (): never {
            throw new \RuntimeException('database down');
        });
        $app->get('/missing', static function (): never {
            throw new HttpException(404, ProblemCode::NotFound, 'Not found.');
        });

        $app->handle(new Request('GET', '/missing', [], [], ''), new Response());
        $app->handle(new Request('GET', '/orders/7', ['token' => 'secret'], [], ''), new Response());

        self::assertCount(1, $this->transport->events);
        $event = $this->transport->events[0];
        self::assertSame('GET /orders/{id}', $event->getTransaction());
        self::assertSame('/orders/{id}', $event->getTags()['http.route']);
        self::assertSame('orders', $event->getTags()['service']);
        self::assertSame(\RuntimeException::class, $event->getExceptions()[0]->getType());
        self::assertStringNotContainsString('secret', (string) json_encode($event->getRequest()));
    }

    public function testRequestScopeDoesNotLeakIntoTheNextRequest(): void
    {
        $app = new App(discoverPackages: false);
        (new SentryReporter($app->container()))->install($app);
        $app->get('/tenant', static function (): never {
            \Sentry\configureScope(static function (Scope $scope): void {
                $scope->setTag('tenant', 'acme');
                $scope->setUser(['id' => '42']);
            });
            throw new \RuntimeException('first');
        });
        $app->get('/other', static function (): never {
            throw new \RuntimeException('second');
        });

        $app->handle(new Request('GET', '/tenant', [], [], ''), new Response());
        $app->handle(new Request('GET', '/other', [], [], ''), new Response());

        self::assertCount(2, $this->transport->events);
        [$first, $second] = $this->transport->events;
        self::assertSame('acme', $first->getTags()['tenant']);
        self::assertArrayNotHasKey('tenant', $second->getTags());
        self::assertNull($second->getUser());
        self::assertSame('orders', $second->getTags()['service'], 'the boot-time scope survives');
    }

    public function testJobFailuresAreReportedAndRethrown(): void
    {
        $handler = new SentryJobHandler(
            static fn (object $job): JobOutcome => throw new \LogicException('bad payload'),
            new SentryReporter(),
        );
        try {
            $handler->handle(new \stdClass());
            self::fail('The job failure must reach the worker.');
        } catch (\LogicException) {
        }
        self::assertSame('stdClass', $this->transport->events[0]->getTags()['job']);
    }
}

final class RecordingTransport implements TransportInterface
{
    /** @var list<Event> */
    public array $events = [];

    public function send(Event $event): Result
    {
        $this->events[] = $event;
        return new Result(ResultStatus::success(), $event);
    }

    public function close(?int $timeout = null): Result
    {
        return new Result(ResultStatus::success());
    }
}
