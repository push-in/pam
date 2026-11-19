<?php

declare(strict_types=1);

namespace Nimbus\Container;

final readonly class Binding
{
    /** @var \Closure(Container): mixed */
    public \Closure $factory;

    /** @param callable(Container): mixed $factory */
    public function __construct(
        callable $factory,
        public BindingLifetime $lifetime,
    ) {
        $this->factory = \Closure::fromCallable($factory);
    }
}

