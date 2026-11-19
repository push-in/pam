<?php

declare(strict_types=1);

namespace Nimbus\Pagination;

/**
 * One page of results with the `meta` and `links` a collection response publishes.
 *
 * @template-covariant T
 */
final readonly class Page
{
    /**
     * @param list<T> $items
     * @param array<string, int|string|null> $meta
     * @param array<string, ?string> $links
     */
    public function __construct(
        public array $items,
        public array $meta,
        public array $links,
    ) {
    }
}
