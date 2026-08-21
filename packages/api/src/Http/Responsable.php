<?php

declare(strict_types=1);

namespace Pam\Api\Http;

use Pam\Http\Request;
use Pam\Http\Response;

interface Responsable
{
    public function toResponse(Request $request, Response $response): Response;
}

