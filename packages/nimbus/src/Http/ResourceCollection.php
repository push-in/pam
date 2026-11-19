<?php

declare(strict_types=1);

namespace Nimbus\Http;

use Pam\Http\Request;
use Pam\Http\Response;

final readonly class ResourceCollection implements Responsable
{
    /**
     * @param list<JsonResource> $resources
     * @param array<string, mixed> $meta
     * @param array<string, ?string> $links
     */
    public function __construct(
        private array $resources,
        private array $meta = [],
        private array $links = [],
    ) {
    }

    /** @param array<string, mixed> $meta */
    public function withMeta(array $meta): self
    {
        return new self($this->resources, $meta, $this->links);
    }

    public function toResponse(Request $request, Response $response): Response
    {
        $payload = [
            'data' => array_map(
                static fn (JsonResource $resource): array => $resource->toArray($request),
                $this->resources,
            ),
        ];
        if ($this->meta !== []) {
            $payload['meta'] = $this->meta;
        }
        if ($this->links !== []) {
            $payload['links'] = $this->links;
        }
        return $response->json($payload);
    }
}
