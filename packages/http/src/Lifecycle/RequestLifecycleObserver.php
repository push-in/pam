<?php

declare(strict_types=1);

namespace Pam\Api\Lifecycle;

use Pam\Http\Request;
use Pam\Http\Response;

interface RequestLifecycleObserver
{
    public function starting(Request $request): void;

    public function finished(Request $request, Response $response, ?\Throwable $failure): void;
}
