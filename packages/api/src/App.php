<?php

declare(strict_types=1);

namespace Pam;

use Pam\Contracts\Http\ApplicationInterface;
use Pam\Contracts\Http\MiddlewareInterface;
use Pam\Contracts\Package\ServiceProviderInterface;
use Pam\Contracts\Transport\TransportApplicationInterface;
use Pam\Contracts\Transport\TransportCapability;
use Pam\Contracts\Transport\TransportProviderInterface;
use Pam\Api\CallableRequestHandler;
use Pam\Api\PackageDiscovery;
use Pam\Api\Pipeline;
use Pam\Api\Router;
use Pam\Api\RoutingResultType;
use Pam\Http\Request;
use Pam\Http\Response;
use Pam\Http\Server as HttpServer;
use Pam\Internal\Runtime;

final class App implements ApplicationInterface, TransportApplicationInterface
{
    private readonly Router $router;

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

    public function __construct(bool $discoverPackages = true)
    {
        $this->router = new Router();
        $this->errorHandler = static function (\Throwable $error, Response $response): Response {
            \Pam\Observability\Telemetry::log('error', 'Unhandled Pam API exception', [
                'exception' => $error::class,
                'message' => $error->getMessage(),
                'file' => $error->getFile(),
                'line' => $error->getLine(),
            ]);
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

    public function get(string $path, callable $handler): self
    {
        return $this->route('GET', $path, $handler);
    }

    public function post(string $path, callable $handler): self
    {
        return $this->route('POST', $path, $handler);
    }

    public function put(string $path, callable $handler): self
    {
        return $this->route('PUT', $path, $handler);
    }

    public function patch(string $path, callable $handler): self
    {
        return $this->route('PATCH', $path, $handler);
    }

    public function delete(string $path, callable $handler): self
    {
        return $this->route('DELETE', $path, $handler);
    }

    public function route(string $method, string $path, callable $handler): self
    {
        $this->assertMutable();
        $this->router->add($method, $path, $handler);
        return $this;
    }

    public function middleware(object|callable $middleware): self
    {
        $this->assertMutable();
        if (interface_exists(\Psr\Http\Server\MiddlewareInterface::class)
            && $middleware instanceof \Psr\Http\Server\MiddlewareInterface
        ) {
            $this->psrMiddleware[] = $middleware;
            return $this;
        }
        if (!$middleware instanceof MiddlewareInterface && !is_callable($middleware)) {
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
        try {
            return $this->pipeline?->handle($request, $response)
                ?? throw new \LogicException('Pam API pipeline was not compiled.');
        } catch (\Throwable $error) {
            $handler = $this->errorHandler;
            $result = $handler($error, $response);
            if (!$result instanceof Response) {
                throw new \UnexpectedValueException('The Pam error handler must return Response.');
            }
            return $result;
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
        $request = $request->withRouteParameters($result->parameters);
        return (new CallableRequestHandler($route->handler))->handle($request, $response);
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
}
