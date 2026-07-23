<?php

declare(strict_types=1);

namespace Pam\Compatibility;

use Closure;
use GuzzleHttp\Psr7\HttpFactory;
use GuzzleHttp\Psr7\Request as GuzzleRequest;
use Illuminate\Container\Container;
use Illuminate\Events\Dispatcher;
use Illuminate\Pipeline\Pipeline;
use Monolog\Handler\TestHandler;
use Monolog\Logger;
use OpenTelemetry\API\Globals;
use Psr\Http\Message\ResponseInterface;
use Psr\Http\Message\ServerRequestInterface;
use Slim\Factory\AppFactory;
use Symfony\Component\EventDispatcher\EventDispatcher;
use Symfony\Component\HttpFoundation\JsonResponse;
use Symfony\Component\HttpFoundation\Request;
use Symfony\Component\HttpFoundation\RequestStack;
use Symfony\Component\HttpKernel\Controller\ArgumentResolver;
use Symfony\Component\HttpKernel\Controller\ControllerResolver;
use Symfony\Component\HttpKernel\HttpKernel;

final class Probe
{
    /** @return array<string, bool|int|string> */
    public static function packages(): array
    {
        $future = \Amp\async(static function (): int {
            \Amp\delay(0.001);
            return 42;
        });
        $answer = function_exists('Pam\\Async\\await')
            ? \Pam\Async\await($future)
            : $future->await();
        if (!is_int($answer)) {
            throw new \UnexpectedValueException('Amp returned a non-integer result.');
        }

        $reactTimer = false;
        \React\EventLoop\Loop::addTimer(0.001, static function () use (&$reactTimer): void {
            $reactTimer = true;
        });
        \React\EventLoop\Loop::run();

        $revoltTimer = false;
        \Revolt\EventLoop::delay(0.001, static function () use (&$revoltTimer): void {
            $revoltTimer = true;
        });
        \Revolt\EventLoop::run();

        $handler = new TestHandler();
        (new Logger('compatibility', [$handler]))->info('Composer logger works');
        $guzzleRequest = new GuzzleRequest('GET', 'https://example.test/health');
        $tracer = Globals::tracerProvider()->getTracer('pam-compatibility');
        $span = $tracer->spanBuilder('probe')->startSpan();
        $span->end();

        return [
            'autoload' => true,
            'amp' => $answer,
            'guzzle' => $guzzleRequest->getUri()->getHost() === 'example.test',
            'illuminateContainer' => self::probeIlluminateContainer(),
            'illuminateEvents' => self::probeIlluminateEvents(),
            'illuminatePipeline' => self::probeIlluminatePipeline(),
            'monolog' => $handler->hasInfoRecords(),
            'pamApi' => class_exists(\Pam\App::class),
            'pamPsrBridge' => class_exists(\Pam\Http\Psr7\Factory::class),
            'pamSocket' => class_exists(\Pam\Socket\Server::class),
            'pamTesting' => class_exists(\Pam\Testing\TestClient::class),
            'otelApi' => class_exists(Globals::class),
            'otelExporter' => class_exists(\OpenTelemetry\Contrib\Otlp\SpanExporter::class),
            'otelSdk' => class_exists(\OpenTelemetry\SDK\Trace\TracerProvider::class),
            'otelTracer' => true,
            'pest' => class_exists(\Pest\TestSuite::class),
            'phpunit' => class_exists(\PHPUnit\Framework\TestCase::class),
            'psr7' => interface_exists(\Psr\Http\Message\ServerRequestInterface::class),
            'psr15' => interface_exists(\Psr\Http\Server\RequestHandlerInterface::class),
            'react' => $reactTimer,
            'revolt' => $revoltTimer,
            'slim' => self::probeSlim(),
            'symfonyHttpKernel' => self::probeSymfonyHttpKernel(),
        ];
    }

    private static function probeIlluminateContainer(): bool
    {
        $container = new Container();
        $container->bind('pam.greeter', static fn (): string => 'container-on-pam');

        return $container->make('pam.greeter') === 'container-on-pam';
    }

    private static function probeIlluminateEvents(): bool
    {
        $dispatcher = new Dispatcher(new Container());
        $received = null;
        $dispatcher->listen('pam.ready', static function (string $payload) use (&$received): void {
            $received = $payload;
        });
        $dispatcher->dispatch('pam.ready', ['events-on-pam']);

        return $received === 'events-on-pam';
    }

    private static function probeIlluminatePipeline(): bool
    {
        $result = (new Pipeline(new Container()))
            ->send('pam')
            ->through([
                static fn (string $payload, Closure $next): mixed => $next(strtoupper($payload)),
            ])
            ->thenReturn();

        return $result === 'PAM';
    }

    private static function probeSlim(): bool
    {
        $factory = new HttpFactory();
        AppFactory::setResponseFactory($factory);
        $app = AppFactory::create();
        $app->get(
            '/hello/{name}',
            static fn (
                ServerRequestInterface $request,
                ResponseInterface $response,
                array $arguments,
            ): ResponseInterface => $response->withHeader('x-pam-name', (string) $arguments['name']),
        );

        $response = $app->handle($factory->createServerRequest('GET', '/hello/runtime'));

        return $response->getStatusCode() === 200
            && $response->getHeaderLine('x-pam-name') === 'runtime';
    }

    private static function probeSymfonyHttpKernel(): bool
    {
        $request = Request::create('/hello?name=runtime');
        $request->attributes->set(
            '_controller',
            static fn (Request $request): JsonResponse => new JsonResponse([
                'name' => $request->query->getString('name'),
            ]),
        );
        $kernel = new HttpKernel(
            new EventDispatcher(),
            new ControllerResolver(),
            new RequestStack(),
            new ArgumentResolver(),
        );

        $response = $kernel->handle($request);

        return $response->getStatusCode() === 200
            && $response->getContent() === '{"name":"runtime"}';
    }
}
