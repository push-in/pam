<?php

declare(strict_types=1);

namespace Pam\Octane;

use Illuminate\Foundation\Application;
use Illuminate\Http\Request;
use Illuminate\Contracts\Config\Repository as ConfigRepository;
use Laravel\Octane\Contracts\Client;
use Laravel\Octane\Octane;
use Laravel\Octane\OctaneResponse;
use Laravel\Octane\RequestContext;
use Pam\Http\Request as PamRequest;
use Pam\Http\Response as PamResponse;
use Throwable;
use UnexpectedValueException;

final class PamClient implements Client
{
    private bool $routeMetrics = false;

    public function __construct(
        private readonly PamRequestMarshaller $requests = new PamRequestMarshaller(),
        private readonly PamResponseEmitter $responses = new PamResponseEmitter(),
    ) {
    }

    public function enableRouteMetrics(bool $enabled): void
    {
        $this->routeMetrics = $enabled;
    }

    /** @return array{Request, RequestContext} */
    public function marshalRequest(RequestContext $context): array
    {
        $nativeRequest = $context['pamRequest'] ?? null;
        if (!$nativeRequest instanceof PamRequest) {
            throw new UnexpectedValueException('The Octane context does not contain a PAM request.');
        }

        $request = $this->requests->marshal($nativeRequest);
        $context['laravelRequest'] = $request;

        return [$request, $context];
    }

    /** @return array{Request, RequestContext} */
    public function marshalNativeRequest(PamResponse $response): array
    {
        $request = $this->requests->capture();
        $context = new RequestContext([
            'laravelRequest' => $request,
            'pamResponse' => $response,
        ]);

        return [$request, $context];
    }

    public function respond(RequestContext $context, OctaneResponse $response): void
    {
        [$request, $target] = $this->context($context);
        $this->responses->emit(
            $request,
            $target,
            $response,
            $this->routeMetrics ? $this->routeTemplate($request) : null,
        );
    }

    public function error(
        Throwable $e,
        Application $app,
        Request $request,
        RequestContext $context,
    ): void {
        [, $target] = $this->context($context);
        $config = $app->make('config');
        if (!$config instanceof ConfigRepository) {
            throw new UnexpectedValueException('Laravel configuration repository is unavailable.');
        }
        $this->responses->emitThrowable(
            request: $request,
            target: $target,
            message: Octane::formatExceptionForClient($e, $config->get('app.debug', false) === true),
            debug: $config->get('app.debug', false) === true,
        );
    }

    /** @return array{Request, PamResponse} */
    private function context(RequestContext $context): array
    {
        $request = $context['laravelRequest'] ?? null;
        $response = $context['pamResponse'] ?? null;

        if (!$request instanceof Request || !$response instanceof PamResponse) {
            throw new UnexpectedValueException('The Octane context is missing its PAM response bridge.');
        }

        return [$request, $response];
    }

    private function routeTemplate(Request $request): ?string
    {
        $route = $request->route();
        if (!is_object($route) || !method_exists($route, 'uri')) {
            return null;
        }
        $uri = $route->uri();
        if (!is_string($uri) || $uri === '' || strlen($uri) > 256) {
            return null;
        }

        return '/'.ltrim($uri, '/');
    }
}
