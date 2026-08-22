<?php

declare(strict_types=1);

namespace Pam\Api\Tests\Lifecycle;

use Pam\App;
use Pam\Api\Lifecycle\RequestLifecycleObserver;
use Pam\Http\Request;
use Pam\Http\Response;
use PHPUnit\Framework\Attributes\CoversClass;
use PHPUnit\Framework\TestCase;

#[CoversClass(App::class)]
final class RequestLifecycleObserverTest extends TestCase
{
    public function testObserversWrapRequestsInDeterministicOrder(): void
    {
        $events = new EventLog();
        $app = new App(discoverPackages: false);
        $app->observe(new RecordingObserver('first', $events));
        $app->observe(new RecordingObserver('second', $events));
        $app->get('/', static fn (Request $request, Response $response): Response => $response->send('ok'));

        $app->handle(new Request('GET', '/', [], [], ''), new Response());

        self::assertSame(['first:start', 'second:start', 'second:finish', 'first:finish'], $events->all());
    }

    public function testObserversSeeHandledFailuresAndCannotBreakCleanup(): void
    {
        $events = new EventLog();
        $app = new App(discoverPackages: false);
        $app->observe(new RecordingObserver('observer', $events, throwDuringFinish: true));
        $app->get('/', static function (): never {
            throw new \RuntimeException('failed request');
        });

        $result = $app->handle(new Request('GET', '/', [], [], ''), new Response())->export();

        self::assertSame(500, $result['status']);
        self::assertSame(['observer:start', 'observer:failure', 'observer:finish'], $events->all());
        self::assertSame(1, $app->container()->diagnostics()['state']);
    }
}

final readonly class RecordingObserver implements RequestLifecycleObserver
{
    public function __construct(
        private string $name,
        private EventLog $events,
        private bool $throwDuringFinish = false,
    ) {
    }

    public function starting(Request $request): void
    {
        $this->events->add("{$this->name}:start");
    }

    public function finished(Request $request, Response $response, ?\Throwable $failure): void
    {
        if ($failure !== null) {
            $this->events->add("{$this->name}:failure");
        }
        $this->events->add("{$this->name}:finish");
        if ($this->throwDuringFinish) {
            throw new \RuntimeException('observer cleanup failed');
        }
    }
}

final class EventLog
{
    /** @var list<string> */
    private array $events = [];

    public function add(string $event): void
    {
        $this->events[] = $event;
    }

    /** @return list<string> */
    public function all(): array
    {
        return $this->events;
    }
}
