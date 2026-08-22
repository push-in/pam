<?php

declare(strict_types=1);

namespace Pam\Api\Middleware;

use Pam\Api\Observability\RequestObservation;
use Pam\Api\Observability\RequestObserver;
use Pam\Api\Container\Container;
use Pam\Api\Route;
use Pam\Contracts\Http\MiddlewareInterface;
use Pam\Contracts\Http\RequestHandlerInterface;
use Pam\Http\Request;
use Pam\Http\Response;

final readonly class ObserveRequestMiddleware implements MiddlewareInterface
{
    public function __construct(
        private RequestObserver $observer,
        private ?Container $container = null,
    )
    {
    }

    public function process(Request $request, Response $response, RequestHandlerInterface $next): Response
    {
        $startedAt = hrtime(true);
        $exception = null;
        try {
            return $next->handle($request, $response);
        } catch (\Throwable $error) {
            $exception = $error::class;
            throw $error;
        } finally {
            $export = $response->export();
            $route = $this->container?->scopedValue(Route::class);
            $this->observer->record(new RequestObservation(
                $request->method,
                $route instanceof Route ? $route->path : $request->path,
                $exception === null ? $export['status'] : 500,
                (hrtime(true) - $startedAt) / 1_000_000_000,
                $exception,
            ));
        }
    }
}
