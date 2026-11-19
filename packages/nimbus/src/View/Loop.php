<?php

declare(strict_types=1);

namespace Nimbus\View;

/** The `$loop` variable of `@foreach` and `@forelse`. */
final class Loop
{
    public int $index = -1;

    public int $iteration = 0;

    public ?int $remaining = null;

    public bool $first = false;

    public bool $last = false;

    public bool $even = false;

    public bool $odd = false;

    public function __construct(
        public readonly ?int $count,
        public readonly int $depth,
        public readonly ?Loop $parent,
    ) {
    }

    /** @internal Advances to the next item. */
    public function advance(): void
    {
        ++$this->index;
        ++$this->iteration;
        $this->first = $this->index === 0;
        $this->odd = $this->iteration % 2 === 1;
        $this->even = !$this->odd;
        if ($this->count !== null) {
            $this->remaining = $this->count - $this->iteration;
            $this->last = $this->iteration === $this->count;
        }
    }
}
