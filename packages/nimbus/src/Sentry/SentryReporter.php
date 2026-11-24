<?php

declare(strict_types=1);

namespace Nimbus\Sentry;

use Nimbus\App;
use Nimbus\Container\Container;
use Nimbus\Http\HttpException;
use Nimbus\Lifecycle\RequestLifecycleObserver;
use Nimbus\Metrics\RequestMetrics;
use Pam\Http\Request;
use Pam\Http\Response;
use Sentry\Event;
use Sentry\EventHint;
use Sentry\SentrySdk;
use Sentry\State\Hub;
use Sentry\State\HubInterface;
use Sentry\State\Scope;

/**
 * Reports the exceptions of a long-lived worker to Sentry (`sentry/sentry`).
 *
 * A PAM worker serves many requests with the same PHP process, so anything a
 * request puts in Sentry's global scope (breadcrumbs, user, tags) would leak
 * into the reports of the next ones. This observer restores the scope that
 * existed at boot whenever the worker has no request in flight, and builds
 * each event from a copy of the scope, tagged with the method, route template
 * and PAM request id of the failed request.
 *
 * Only failures that become 5xx responses are reported: `HttpException` with
 * a 4xx status is a client error and a cancelled request is not a failure.
 * Query strings, bodies, cookies and headers are never sent.
 */
final class SentryReporter implements RequestLifecycleObserver
{
    private int $inFlight = 0;

    private readonly Scope $baseline;

    /** Works on `SentrySdk::getCurrentHub()`: initialize the SDK (`\Sentry\init()`) first. */
    public function __construct(private readonly ?Container $container = null)
    {
        $this->baseline = self::copyScope(SentrySdk::getCurrentHub());
    }

    /**
     * Initializes the SDK from `SENTRY_DSN`, `SENTRY_ENVIRONMENT` (or `APP_ENV`)
     * and `SENTRY_RELEASE`; null when sentry/sentry is missing or no DSN is set.
     */
    public static function fromEnvironment(?Container $container = null): ?self
    {
        $dsn = getenv('SENTRY_DSN');
        if (!function_exists('Sentry\\init') || !is_string($dsn) || $dsn === '') {
            return null;
        }
        $environment = getenv('SENTRY_ENVIRONMENT') ?: getenv('APP_ENV') ?: getenv('PAM_ENV');
        $release = getenv('SENTRY_RELEASE');
        \Sentry\init(array_filter([
            'dsn' => $dsn,
            'environment' => is_string($environment) ? $environment : null,
            'release' => is_string($release) && $release !== '' ? $release : null,
            'send_default_pii' => false,
        ], static fn (mixed $value): bool => $value !== null));
        return new self($container);
    }

    public function install(App $app): self
    {
        $app->observe($this);
        return $this;
    }

    public function starting(Request $request): void
    {
        ++$this->inFlight;
    }

    public function finished(Request $request, Response $response, ?\Throwable $failure): void
    {
        try {
            if ($failure !== null && self::reportable($failure)) {
                $this->report($request, $failure);
            }
        } finally {
            if (--$this->inFlight === 0) {
                $this->reset();
            }
        }
    }

    /** Reports an exception with a copy of the current scope and optional tags. */
    /** @param array<string, scalar> $tags */
    public function capture(\Throwable $error, array $tags = []): void
    {
        $hub = SentrySdk::getCurrentHub();
        $client = $hub->getClient();
        if ($client === null) {
            return;
        }
        $scope = self::copyScope($hub);
        foreach ($tags as $name => $value) {
            $scope->setTag((string) $name, (string) $value);
        }
        $client->captureException($error, $scope, EventHint::fromArray(['exception' => $error]));
    }

    private function report(Request $request, \Throwable $failure): void
    {
        $route = RequestMetrics::route($this->container);
        $requestId = $_SERVER['PAM_REQUEST_ID'] ?? null;
        $tags = ['http.method' => $request->method, 'http.route' => $route];
        if (is_string($requestId) && $requestId !== '') {
            $tags['request_id'] = $requestId;
        }
        $hub = SentrySdk::getCurrentHub();
        $client = $hub->getClient();
        if ($client === null) {
            return;
        }
        $scope = self::copyScope($hub);
        $scope->setTags($tags);
        $scope->addEventProcessor(static function (Event $event) use ($request, $route): Event {
            $event->setTransaction($request->method . ' ' . $route);
            $event->setRequest(['method' => $request->method, 'url' => $request->path]);
            return $event;
        });
        $client->captureException($failure, $scope, EventHint::fromArray(['exception' => $failure]));
    }

    private static function copyScope(HubInterface $hub): Scope
    {
        $copy = new Scope();
        $hub->configureScope(static function (Scope $scope) use (&$copy): void {
            $copy = clone $scope;
        });
        return $copy;
    }

    /** Puts back the boot-time scope: nothing a request added survives it. */
    private function reset(): void
    {
        $client = SentrySdk::getCurrentHub()->getClient();
        SentrySdk::setCurrentHub(new Hub($client, clone $this->baseline));
    }

    private static function reportable(\Throwable $failure): bool
    {
        if ($failure::class === 'Pam\\Internal\\HttpRequestCancelled') {
            return false;
        }
        return !$failure instanceof HttpException || $failure->status >= 500;
    }
}
