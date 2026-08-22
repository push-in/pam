<?php

declare(strict_types=1);

namespace Pam\Contracts\Http;

use Pam\Http\Request;
use Pam\Http\Response;

interface MiddlewareInterface
{
    public function process(
        Request $request,
        Response $response,
        RequestHandlerInterface $next,
    ): Response;
}
