<?php

declare(strict_types=1);

namespace Pam\Contracts\Http;

use Pam\Http\Request;
use Pam\Http\Response;

interface ApplicationInterface
{
    public function route(string $method, string $path, callable $handler): self;

    public function middleware(object|callable $middleware): self;

    public function onError(callable $handler): self;

    public function handle(Request $request, Response $response): Response;
}
