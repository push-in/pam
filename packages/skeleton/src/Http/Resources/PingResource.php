<?php

declare(strict_types=1);

namespace App\Http\Resources;

use App\Services\ReadinessSnapshot;
use Pam\Api\Http\JsonResource;
use Pam\Http\Request;

final readonly class PingResource extends JsonResource
{
    public function toArray(Request $request): array
    {
        if (!$this->resource instanceof ReadinessSnapshot) {
            throw new \LogicException('PingResource requires a ReadinessSnapshot.');
        }
        return [
            'status' => $this->resource->status->value,
            'message' => $this->resource->message,
            'requestId' => $this->resource->requestId,
        ];
    }
}
