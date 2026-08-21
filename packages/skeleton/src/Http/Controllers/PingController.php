<?php

declare(strict_types=1);

namespace App\Http\Controllers;

use App\Http\Resources\PingResource;
use App\Services\ReadinessService;
use Pam\Http\Request;

final readonly class PingController
{
    public function __construct(private ReadinessService $readiness)
    {
    }

    public function show(Request $request): PingResource
    {
        return new PingResource($this->readiness->snapshot(
            $request->getHeader('x-request-id'),
        ));
    }
}
