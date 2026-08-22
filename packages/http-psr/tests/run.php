<?php

declare(strict_types=1);

use Pam\Http\Psr7\Factory;
use Pam\Http\Psr15\Pipeline;
use Psr\Http\Message\ResponseInterface;
use Psr\Http\Message\ServerRequestInterface;
use Psr\Http\Server\RequestHandlerInterface;

require dirname(__DIR__) . '/vendor/autoload.php';

$factory = new Factory();
$request = $factory->createServerRequest('GET', 'https://example.test/users/42');
$handler = new class($factory) implements RequestHandlerInterface {
    public function __construct(private readonly Factory $factory)
    {
    }

    public function handle(ServerRequestInterface $request): ResponseInterface
    {
        return $this->factory->createResponse(204)->withHeader('x-path', $request->getUri()->getPath());
    }
};

$response = (new Pipeline($handler))->handle($request);
if ($response->getStatusCode() !== 204 || $response->getHeaderLine('x-path') !== '/users/42') {
    throw new RuntimeException('PSR-7/15 interoperability smoke failed.');
}

fwrite(STDOUT, "pam-http-psr-ok\n");
