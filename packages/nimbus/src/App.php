<?php

declare(strict_types=1);

namespace Nimbus;

use Pam\Contracts\Http\ApplicationInterface;
use Pam\Contracts\Http\MiddlewareInterface;
use Pam\Contracts\Package\ServiceProviderInterface;
use Pam\Contracts\Transport\TransportCapability;
use Pam\Contracts\Transport\TransportProviderInterface;
use Nimbus\CallableRequestHandler;
use Nimbus\Container\Container;
use Nimbus\HandlerResolver;
use Nimbus\Http\HttpException;
use Nimbus\PackageDiscovery;
use Nimbus\PendingRoute;
use Nimbus\Pipeline;
use Nimbus\Router;
use Nimbus\RoutingResultType;
use Nimbus\RouteRegistrar;
use Nimbus\OpenApi\OpenApiGenerator;
use Nimbus\Lifecycle\RequestLifecycleObserver;
use Nimbus\Runtime\Background;
use Pam\Http\Request;
use Pam\Http\Response;
use Pam\Http\Server as HttpServer;
use Pam\Internal\Runtime;

final class App implements ApplicationInterface
{
    private readonly Router $router;

    private readonly Container $container;

    private readonly HandlerResolver $handlerResolver;

    /** @var list<MiddlewareInterface|callable> */
    private array $middleware = [];

    /** @var list<ServiceProviderInterface> */
    private array $providers = [];

    /** @var array<string, TransportProviderInterface> */
    private array $transports = [];

    private ?Pipeline $pipeline = null;

    private ?object $psrHandler = null;

    /** @var list<object> */
    private array $psrMiddleware = [];

    private \Closure $errorHandler;

    private bool $frozen = false;

    /** @var list<RequestLifecycleObserver> */
    private array $observers = [];

    private ?Background $background = null;

    public function __construct(bool $discoverPackages = true, ?Container $container = null)
    {
        $this->container = $container ?? new Container();
        $this->router = new Router($this->container);
        $this->handlerResolver = new HandlerResolver($this->container);
        $this->container->instance(self::class, $this);
        $this->container->instance(Container::class, $this->container);
        // PSR-3 logging goes to the runtime's structured log unless the application binds its own.
        if (interface_exists(\Psr\Log\LoggerInterface::class) && class_exists(\Pam\Observability\Logger::class)) {
            $this->container->singleton(\Psr\Log\LoggerInterface::class, \Pam\Observability\Logger::class);
        }
        // One authorization gate per application, used by can() routes.
        $this->container->singleton(\Nimbus\Auth\Gate::class);
        // Signed routes use APP_KEY unless the application binds its own signer.
        $this->container->singleton(
            \Nimbus\Security\UrlSigner::class,
            static fn (): \Nimbus\Security\UrlSigner => \Nimbus\Security\UrlSigner::fromEnvironment(),
        );
        $this->errorHandler = static function (\Throwable $error, Response $response): Response {
            \Pam\Observability\Telemetry::log('error', 'Unhandled Nimbus exception', [
                'exception' => $error::class,
                'message' => $error->getMessage(),
                'file' => $error->getFile(),
                'line' => $error->getLine(),
            ]);
            return self::problemResponse(
                $response,
                500,
                \Nimbus\Http\ProblemCode::InternalError,
                'Internal Server Error',
            );
        };

        if ($discoverPackages) {
            $root = getcwd();
            if (is_string($root)) {
                foreach (PackageDiscovery::providers($root) as $providerClass) {
                    $this->provider(new $providerClass());
                }
            }
        }
    }

    /** @param callable|class-string|array{class-string, non-empty-string} $handler */
    public function get(string $path, callable|string|array $handler): PendingRoute
    {
        return $this->registerRoute('GET', $path, $handler);
    }

    /** @param callable|class-string|array{class-string, non-empty-string} $handler */
    public function post(string $path, callable|string|array $handler): PendingRoute
    {
        return $this->registerRoute('POST', $path, $handler);
    }

    /** @param callable|class-string|array{class-string, non-empty-string} $handler */
    public function put(string $path, callable|string|array $handler): PendingRoute
    {
        return $this->registerRoute('PUT', $path, $handler);
    }

    /** @param callable|class-string|array{class-string, non-empty-string} $handler */
    public function patch(string $path, callable|string|array $handler): PendingRoute
    {
        return $this->registerRoute('PATCH', $path, $handler);
    }

    /** @param callable|class-string|array{class-string, non-empty-string} $handler */
    public function delete(string $path, callable|string|array $handler): PendingRoute
    {
        return $this->registerRoute('DELETE', $path, $handler);
    }

    /** @param callable|class-string|array{class-string, non-empty-string} $handler */
    public function head(string $path, callable|string|array $handler): PendingRoute
    {
        return $this->registerRoute('HEAD', $path, $handler);
    }

    /** @param callable|class-string|array{class-string, non-empty-string} $handler */
    public function options(string $path, callable|string|array $handler): PendingRoute
    {
        return $this->registerRoute('OPTIONS', $path, $handler);
    }

    /** @param callable|class-string|array{class-string, non-empty-string} $handler */
    public function route(string $method, string $path, callable|string|array $handler): self
    {
        $this->registerRoute($method, $path, $handler);
        return $this;
    }

    public function container(): Container
    {
        return $this->container;
    }

    /** The disks registered by `Storage\StorageServiceProvider`. */
    public function storage(): \Nimbus\Storage\StorageManager
    {
        try {
            $storage = $this->container->get(\Nimbus\Storage\StorageManager::class);
        } catch (\RuntimeException $error) {
            throw new \LogicException('Register Nimbus\\Storage\\StorageServiceProvider to use storage().', 0, $error);
        }
        return $storage instanceof \Nimbus\Storage\StorageManager
            ? $storage
            : throw new \LogicException('Register Nimbus\\Storage\\StorageServiceProvider to use storage().');
    }

    /** The authorization gate: abilities, policies and before() hooks checked by `can()` routes. */
    public function gate(): \Nimbus\Auth\Gate
    {
        $gate = $this->container->get(\Nimbus\Auth\Gate::class);
        return $gate instanceof \Nimbus\Auth\Gate ? $gate : throw new \LogicException('The container binds another Gate.');
    }

    /** Boot providers and freeze application configuration without serving a request. */
    public function boot(): self
    {
        $this->freeze();
        return $this;
    }

    public function prefix(string $prefix): RouteRegistrar
    {
        return new RouteRegistrar($this, $prefix);
    }

    /** @param callable(RouteRegistrar): void $routes */
    public function group(callable $routes): void
    {
        (new RouteRegistrar($this))->group($routes);
    }

    /**
     * @param class-string $controller
     * @param callable(RouteRegistrar): void $routes
     */
    public function controller(string $controller, callable $routes): void
    {
        (new RouteRegistrar($this))->controller($controller, $routes);
    }

    /**
     * @param class-string $controller
     * @param ?callable(RouteRegistrar): void $routes
     */
    public function resource(string $path, string $controller, ?callable $routes = null): void
    {
        (new RouteRegistrar($this))->resource($path, $controller, $routes);
    }

    /** Loads the application's routes file, which receives the router as `$router`. */
    public function routes(string $file): self
    {
        (new RouteRegistrar($this))->load($file);
        return $this;
    }

    public function openApi(string $title = 'Nimbus API', string $version = '1.0.0'): OpenApiGenerator
    {
        return new OpenApiGenerator($this->router, $title, $version);
    }

    /**
     * Serves the OpenAPI document at `{path}/openapi.json` and an API reference page at `{path}`.
     * The document is generated on the first request and kept for the worker's lifetime.
     */
    public function docs(string $path = '/docs', string $title = 'Nimbus API', string $version = '1.0.0'): self
    {
        $path = '/' . trim($path, '/');
        $document = null;
        $this->get($path . '/openapi.json', function (Request $request, Response $response) use (&$document, $title, $version): Response {
            $document ??= $this->openApi($title, $version)->toJson();
            return $response->header('content-type', 'application/json')->send($document);
        });
        $html = '<!doctype html><html><head><meta charset="utf-8"><title>' . htmlspecialchars($title) . '</title>'
            . '<meta name="viewport" content="width=device-width, initial-scale=1"></head><body>'
            . '<script id="api-reference" data-url="' . htmlspecialchars($path . '/openapi.json') . '"></script>'
            . '<script src="https://cdn.jsdelivr.net/npm/@scalar/api-reference"></script></body></html>';
        $this->get($path, static fn (Request $request, Response $response): Response => $response
            ->header('content-type', 'text/html; charset=utf-8')
            ->send($html));
        return $this;
    }

    /** @param MiddlewareInterface|callable|class-string<MiddlewareInterface> $middleware */
    public function middleware(object|callable|string $middleware): self
    {
        $this->assertMutable();
        if (is_string($middleware)) {
            if (is_a($middleware, MiddlewareInterface::class, true)) {
                $middleware = new \Nimbus\ContainerMiddleware($this->container, $middleware);
            }
        }
        if (interface_exists(\Psr\Http\Server\MiddlewareInterface::class)
            && $middleware instanceof \Psr\Http\Server\MiddlewareInterface
        ) {
            $this->psrMiddleware[] = $middleware;
            return $this;
        }
        if (is_object($middleware) && !$middleware instanceof MiddlewareInterface && !method_exists($middleware, '__invoke')) {
            throw new \InvalidArgumentException('Middleware must implement a Pam/PSR contract or be callable.');
        }
        $this->middleware[] = $middleware;
        return $this;
    }

    public function handler(object|callable $handler): self
    {
        $this->assertMutable();
        $this->psrHandler = is_object($handler) ? $handler : \Closure::fromCallable($handler);
        return $this;
    }

    public function provider(ServiceProviderInterface $provider): self
    {
        $this->assertMutable();
        $provider->register($this);
        $this->providers[] = $provider;
        return $this;
    }

    public function transport(TransportProviderInterface $provider): self
    {
        $this->assertMutable();
        $descriptor = $provider->descriptor();
        if (!$descriptor->supports(TransportCapability::Publish)
            && !$descriptor->supports(TransportCapability::Consume)
        ) {
            throw new \InvalidArgumentException(
                "Transport {$descriptor->id} must support publishing or consuming.",
            );
        }
        if (isset($this->transports[$descriptor->id])) {
            throw new \LogicException("Transport {$descriptor->id} is already registered.");
        }
        $this->transports[$descriptor->id] = $provider;
        ksort($this->transports, SORT_STRING);
        return $this;
    }

    /** @return array<string, TransportProviderInterface> */
    public function transports(): array
    {
        return $this->transports;
    }

    public function onError(callable $handler): self
    {
        $this->assertMutable();
        $this->errorHandler = \Closure::fromCallable($handler);
        return $this;
    }

    public function observe(RequestLifecycleObserver $observer): self
    {
        $this->assertMutable();
        $this->observers[] = $observer;
        return $this;
    }

    /**
     * Queue workers and the scheduler of this application. They run on the
     * ticks PAM sends to task workers (`--task-workers N`), or in a dedicated
     * process through Background::run().
     */
    public function background(): Background
    {
        return $this->background ??= new Background();
    }

    /** @param array<string, mixed> $options */
    public function listen(int $port, string $host = '127.0.0.1', array $options = []): void
    {
        $this->freeze();
        if ($this->psrHandler !== null) {
            Runtime::registerPsrHandler($this->psrHandler);
            foreach ($this->psrMiddleware as $middleware) {
                Runtime::registerMiddleware($middleware);
            }
            Runtime::listen($port, $host, $options);
            return;
        }
        foreach ($this->router->routes() as $route) {
            Runtime::describeRoute($route->method, $route->path);
        }
        HttpServer::create($this->handle(...))->listen($port, $host, $options);
    }

    public function handle(Request $request, Response $response): Response
    {
        $this->freeze();
        $this->container->beginScope();
        $this->container->scopedInstance(Request::class, $request);
        $this->container->scopedInstance(Response::class, $response);
        $failure = null;
        $startedObservers = [];
        try {
            foreach ($this->observers as $observer) {
                $observer->starting($request);
                $startedObservers[] = $observer;
            }
            if ($this->background !== null) {
                $answered = $this->background->answer($request, $response);
                if ($answered !== null) {
                    return $answered;
                }
            }
            $handled = $this->pipeline?->handle($request, $response)
                ?? throw new \LogicException('Nimbus pipeline was not compiled.');
            return $this->finalizeResponse($request, $handled);
        } catch (\Throwable $error) {
            $failure = $error;
            // The client went away: the runtime answers 499 and nothing is logged as an error.
            for ($cause = $error; $cause !== null; $cause = $cause->getPrevious()) {
                if ($cause::class === 'Pam\\Internal\\HttpRequestCancelled') {
                    throw $cause;
                }
            }
            if ($error instanceof HttpException) {
                return $this->finalizeResponse($request, self::problemResponse(
                    $response,
                    $error->status,
                    $error->problemCode,
                    $error->getMessage(),
                    $error->details,
                ));
            }
            $handler = $this->errorHandler;
            $result = $handler($error, $response);
            if (!$result instanceof Response) {
                throw new \UnexpectedValueException('The Pam error handler must return Response.');
            }
            return $this->finalizeResponse($request, $result);
        } finally {
            foreach (array_reverse($startedObservers) as $observer) {
                try {
                    $observer->finished($request, $response, $failure);
                } catch (\Throwable $observerError) {
                    \Pam\Observability\Telemetry::log('error', 'PAM request observer failed during cleanup', [
                        'observer' => $observer::class,
                        'exception' => $observerError::class,
                        'message' => $observerError->getMessage(),
                    ]);
                }
            }
            $this->container->endScope();
        }
    }

    private function finalizeResponse(Request $request, Response $response): Response
    {
        return $request->method === 'HEAD' ? $response->send(null) : $response;
    }

    /** @param array<string, mixed> $details */
    private static function problemResponse(
        Response $response,
        int $status,
        \Nimbus\Http\ProblemCode $code,
        string $title,
        array $details = [],
    ): Response {
        return $response
            ->json([
                ...$details,
                'type' => 'https://pam.dev/problems/' . $code->value,
                'title' => $title,
                'status' => $status,
                'code' => $code->value,
            ], $status)
            ->header('content-type', 'application/problem+json; charset=utf-8');
    }

    private function dispatchRoute(Request $request, Response $response): Response
    {
        $result = $this->router->match($request->method, $request->path);
        if ($result->type === RoutingResultType::NotFound) {
            throw new HttpException(404, \Nimbus\Http\ProblemCode::NotFound, 'Route not found.');
        }
        if ($result->type === RoutingResultType::MethodNotAllowed) {
            if ($request->method === 'OPTIONS') {
                return $response
                    ->status(204)
                    ->header('allow', implode(', ', $result->allowedMethods));
            }
            $response->header('allow', implode(', ', $result->allowedMethods));
            throw new HttpException(
                405,
                \Nimbus\Http\ProblemCode::MethodNotAllowed,
                'Method Not Allowed',
            );
        }
        $route = $result->route ?? throw new \LogicException('A matched route must contain a handler.');
        $this->container->scopedInstance(\Nimbus\Route::class, $route);
        $request = $request->withRouteParameters($result->parameters);
        $destination = new CallableRequestHandler($route->handler);
        if ($route->middleware === []) {
            return $destination->handle($request, $response);
        }
        return (new Pipeline($route->middleware, $destination))->handle($request, $response);
    }

    private function freeze(): void
    {
        if ($this->frozen) {
            return;
        }
        foreach ($this->providers as $provider) {
            $provider->boot($this);
        }
        $this->pipeline = new Pipeline(
            $this->middleware,
            new CallableRequestHandler($this->dispatchRoute(...)),
        );
        $this->frozen = true;
    }

    private function assertMutable(): void
    {
        if ($this->frozen) {
            throw new \LogicException('Pam application configuration is frozen after it starts handling requests.');
        }
    }

    /**
     * @param callable|class-string|array{class-string, non-empty-string} $handler
     */
    private function registerRoute(string $method, string $path, callable|string|array $handler): PendingRoute
    {
        $this->assertMutable();
        // `products` and `/products` are the same route.
        if ($path !== '' && $path[0] !== '/') {
            $path = '/' . $path;
        }
        $route = $this->router->register($method, $path, $this->handlerResolver->resolve($handler));
        $route->sourceHandler = $handler;
        return new PendingRoute($this, $this->router, $route);
    }
}
