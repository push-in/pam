<?php

declare(strict_types=1);

use Pam\App;
use Pam\Http\Psr7\Factory;
use Psr\Http\Message\ResponseInterface;
use Psr\Http\Message\ServerRequestInterface;
use Psr\Http\Server\MiddlewareInterface;
use Psr\Http\Server\RequestHandlerInterface;

$factory = new Factory();
$app = new App();
$app->middleware(new class implements MiddlewareInterface {
    public function process(ServerRequestInterface $request, RequestHandlerInterface $handler): ResponseInterface
    {
        return $handler->handle($request)->withHeader('x-pam-middleware', 'active');
    }
});
$app->handler(new class($factory) implements RequestHandlerInterface {
    public function __construct(private readonly Factory $factory)
    {
    }

    public function handle(ServerRequestInterface $request): ResponseInterface
    {
        if ($request->getUri()->getPath() === '/amp') {
            $future = \Amp\async(static function (): string {
                \Amp\delay(0.01);
                return 'amp-on-pam';
            });
            $payload = json_encode([
                'result' => \Pam\Async\await($future),
                'revolt' => \Revolt\EventLoop::getDriver()::class,
            ], JSON_THROW_ON_ERROR | JSON_UNESCAPED_SLASHES);
            return $this->factory
                ->createResponse(200)
                ->withHeader('content-type', 'application/json')
                ->withBody($this->factory->createStream($payload));
        }

        $payload = json_encode([
            'body' => $request->getParsedBody(),
            'cookie' => $request->getCookieParams()['session'] ?? null,
            'method' => $request->getMethod(),
            'query' => $request->getQueryParams(),
            'requestId' => $request->getServerParams()['PAM_REQUEST_ID'] ?? null,
        ], JSON_THROW_ON_ERROR | JSON_UNESCAPED_SLASHES);

        return $this->factory
            ->createResponse(201)
            ->withHeader('content-type', 'application/json')
            ->withAddedHeader('set-cookie', 'first=1; Path=/')
            ->withAddedHeader('set-cookie', 'second=2; Path=/')
            ->withBody($this->factory->createStream($payload));
    }
});
$app->listen((int) ($argv[1] ?? 3000));
