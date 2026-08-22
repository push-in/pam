<?php

declare(strict_types=1);

namespace Pam\Contracts\Http;

use Pam\Http\Request;
use Pam\Http\Response;

interface RequestHandlerInterface
{
    public function handle(Request $request, Response $response): Response;
}
