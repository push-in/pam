<?php

declare(strict_types=1);

namespace Pam\Laravel\Contracts;

use Illuminate\Http\Request;
use Symfony\Component\HttpFoundation\Response;

interface LifecycleHook
{
    public function beforeRequest(Request $request): void;

    public function afterRequest(Request $request, Response $response, int $durationNanoseconds): void;
}
