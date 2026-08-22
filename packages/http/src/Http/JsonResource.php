<?php

declare(strict_types=1);

namespace Pam\Api\Http;

use Pam\Http\Request;
use Pam\Http\Response;

abstract readonly class JsonResource implements Responsable
{
    final public function __construct(
        protected mixed $resource,
        private int $status = 200,
    ) {
        if ($this->status < 100 || $this->status > 599) {
            throw new \InvalidArgumentException('Resource status must be a valid HTTP status code.');
        }
    }

    /** @return array<string, mixed> */
    abstract public function toArray(Request $request): array;

    public function toResponse(Request $request, Response $response): Response
    {
        return $response->json(['data' => $this->toArray($request)], $this->status);
    }

    /** @param iterable<mixed> $resources */
    public static function collection(iterable $resources): ResourceCollection
    {
        $items = [];
        foreach ($resources as $resource) {
            $items[] = new static($resource);
        }
        return new ResourceCollection($items);
    }
}
