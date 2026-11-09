<?php

declare(strict_types=1);

namespace Pam\Octane;

use Illuminate\Foundation\Application;
use Illuminate\Http\Request;
use Illuminate\Contracts\Config\Repository as ConfigRepository;
use Laravel\Octane\Contracts\Client;
use Laravel\Octane\Contracts\ServesStaticFiles;
use Laravel\Octane\MimeType;
use Laravel\Octane\Octane;
use Laravel\Octane\OctaneResponse;
use Laravel\Octane\RequestContext;
use Pam\Http\Request as PamRequest;
use Pam\Http\Response as PamResponse;
use Symfony\Component\HttpFoundation\BinaryFileResponse;
use Throwable;
use UnexpectedValueException;

final class PamClient implements Client, ServesStaticFiles
{
    private bool $routeMetrics = false;

    private ?string $publicPath = null;

    /** @var array<string, array<string, string>> */
    private array $staticFileHeaders = [];

    public function __construct(
        private readonly PamRequestMarshaller $requests = new PamRequestMarshaller(),
        private readonly PamResponseEmitter $responses = new PamResponseEmitter(),
    ) {
    }

    public function enableRouteMetrics(bool $enabled): void
    {
        $this->routeMetrics = $enabled;
    }

    /**
     * Serve files below the public directory (Vite builds, robots.txt, the
     * storage symlink) like Octane's Swoole server does.
     *
     * @param array<string, array<string, string>> $headers glob pattern => headers
     */
    public function serveStaticFilesFrom(?string $publicPath, array $headers = []): void
    {
        $resolved = $publicPath === null ? false : realpath($publicPath);
        $this->publicPath = $resolved === false ? null : $resolved;
        $this->staticFileHeaders = $headers;
    }

    public function canServeRequestAsStaticFile(Request $request, RequestContext $context): bool
    {
        if ($this->publicPath === null
            || !in_array($request->getMethod(), ['GET', 'HEAD'], true)
            || $request->path() === '/') {
            return false;
        }

        return $this->staticFilePath($request) !== null;
    }

    public function serveStaticFile(Request $request, RequestContext $context): void
    {
        [, $target] = $this->context($context);
        $path = $this->staticFilePath($request);
        if ($path === null) {
            throw new UnexpectedValueException('The requested static file is no longer servable.');
        }

        $response = new BinaryFileResponse($path, 200, [
            'Content-Type' => MimeType::get(pathinfo($path, PATHINFO_EXTENSION)),
        ], true, null, true, true);
        $explicitlyPrivate = false;
        foreach ($this->staticFileHeaders as $pattern => $headers) {
            if ($request->is($pattern)) {
                $response->headers->add($headers);
                foreach ($headers as $name => $value) {
                    $explicitlyPrivate = $explicitlyPrivate
                        || (strtolower($name) === 'cache-control' && str_contains(strtolower($value), 'private'));
                }
            }
        }
        if (!$explicitlyPrivate) {
            $response->setPublic();
        }
        $response->prepare($request);
        if ($response->isNotModified($request)) {
            $target->status(304);
            $target->send('');

            return;
        }

        $this->responses->emit($request, $target, new OctaneResponse($response));
    }

    /**
     * The real path of a servable file, or null. Symlinks inside public/
     * (`storage:link`) are followed; nothing outside them is reachable.
     */
    private function staticFilePath(Request $request): ?string
    {
        $publicPath = $this->publicPath;
        if ($publicPath === null) {
            return null;
        }
        $relative = rawurldecode($request->path());
        if ($relative === '' || str_contains($relative, "\0")) {
            return null;
        }
        $candidate = $publicPath.DIRECTORY_SEPARATOR.$relative;
        $real = realpath($candidate);
        if ($real === false || !is_file($real)) {
            return null;
        }
        $extension = strtolower(pathinfo($real, PATHINFO_EXTENSION));
        if (in_array($extension, ['php', 'phar', 'phtml', 'htaccess', 'config'], true)
            || str_starts_with(basename($real), '.')) {
            return null;
        }
        if (str_starts_with($real, $publicPath.DIRECTORY_SEPARATOR)) {
            return $real;
        }

        // Allowed only through a symlink that lives directly in public/.
        $first = explode('/', ltrim($relative, '/'))[0];
        $link = $publicPath.DIRECTORY_SEPARATOR.$first;
        if (!is_link($link)) {
            return null;
        }
        $target = realpath($link);
        if ($target === false || !str_starts_with($real, rtrim($target, DIRECTORY_SEPARATOR).DIRECTORY_SEPARATOR)) {
            return null;
        }
        if (str_contains($relative, '..')) {
            return null;
        }

        return $real;
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
