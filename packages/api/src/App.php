<?php

declare(strict_types=1);

namespace Pam;

use Pam\Contracts\Http\ApplicationInterface;
use Pam\Contracts\Http\MiddlewareInterface;
use Pam\Contracts\Package\ServiceProviderInterface;
use Pam\Api\CallableRequestHandler;
use Pam\Api\Container\Container;
use Pam\Api\HandlerResolver;
use Pam\Api\Http\HttpException;
use Pam\Api\PackageDiscovery;
use Pam\Api\PendingRoute;
use Pam\Api\Pipeline;
use Pam\Api\Router;
use Pam\Api\RoutingResultType;
use Pam\Api\RouteRegistrar;
use Pam\Api\OpenApi\OpenApiGenerator;
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

    private ?Pipeline $pipeline = null;

    private ?object $psrHandler = null;

    /** @var list<object> */
    private array $psrMiddleware = [];

    private \Closure $errorHandler;

    private bool $frozen = false;

    public function __construct(bool $discoverPackages = true, ?Container $container = null)
    {
        $this->container = $container ?? new Container();
        $this->router = new Router($this->container);
        $this->handlerResolver = new HandlerResolver($this->container);
        $this->container->instance(self::class, $this);
        $this->container->instance(Container::class, $this->container);
        $this->errorHandler = static function (\Throwable $error, Response $response): Response {
            $telemetry = ['Pam\\Observability\\Telemetry', 'log'];
            if (is_callable($telemetry)) {
                $telemetry('error', 'Unhandled Pam API exception', [
                    'exception' => $error::class,
                    'message' => $error->getMessage(),
                    'file' => $error->getFile(),
                    'line' => $error->getLine(),
                ]);
            }
            return $response->json(['error' => 'Internal Server Error'], 500);
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
    public function route(string $method, string $path, callable|string|array $handler): self
    {
        $this->registerRoute($method, $path, $handler);
        return $this;
    }

    public function container(): Container
    {
        return $this->container;
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

    public function openApi(string $title = 'PAM API', string $version = '1.0.0'): OpenApiGenerator
    {
        return new OpenApiGenerator($this->router, $title, $version);
    }

    /** @param MiddlewareInterface|callable|class-string<MiddlewareInterface> $middleware */
    public function middleware(object|callable|string $middleware): self
    {
        $this->assertMutable();
        if (is_string($middleware)) {
            if (is_a($middleware, MiddlewareInterface::class, true)) {
                $middleware = new \Pam\Api\ContainerMiddleware($this->container, $middleware);
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

    public function onError(callable $handler): self
    {
        $this->assertMutable();
        $this->errorHandler = \Closure::fromCallable($handler);
        return $this;
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
        try {
            return $this->pipeline?->handle($request, $response)
                ?? throw new \LogicException('Pam API pipeline was not compiled.');
        } catch (\Throwable $error) {
            if ($error instanceof HttpException) {
                return $response->json([
                    'type' => 'https://pam.dev/problems/' . $error->problemCode->value,
                    'title' => $error->getMessage(),
                    'status' => $error->status,
                    'code' => $error->problemCode->value,
                    ...$error->details,
                ], $error->status);
            }
            $handler = $this->errorHandler;
            $result = $handler($error, $response);
            if (!$result instanceof Response) {
                throw new \UnexpectedValueException('The Pam error handler must return Response.');
            }
            return $result;
        } finally {
            $this->container->endScope();
        }
    }

    private function dispatchRoute(Request $request, Response $response): Response
    {
        $result = $this->router->match($request->method, $request->path);
        if ($result->type === RoutingResultType::NotFound) {
            return $response->json(['error' => 'Route not found'], 404);
        }
        if ($result->type === RoutingResultType::MethodNotAllowed) {
            return $response
                ->header('allow', implode(', ', $result->allowedMethods))
                ->json(['error' => 'Method Not Allowed'], 405);
        }
        $route = $result->route ?? throw new \LogicException('A matched route must contain a handler.');
        $this->container->scopedInstance(\Pam\Api\Route::class, $route);
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
        $route = $this->router->register($method, $path, $this->handlerResolver->resolve($handler));
        $route->sourceHandler = $handler;
        return new PendingRoute($this, $this->router, $route);
    }
}
