<?php

declare(strict_types=1);

namespace Pam\Api\Http;

use Pam\Http\Request;
use Pam\Http\Response;

abstract readonly class JsonResource implements Responsable
{
    final public function __construct(protected mixed $resource)
    {
    }

    /** @return array<string, mixed> */
    abstract public function toArray(Request $request): array;

    public function toResponse(Request $request, Response $response): Response
    {
        return $response->json(['data' => $this->toArray($request)]);
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
