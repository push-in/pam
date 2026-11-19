<?php

declare(strict_types=1);

namespace Nimbus\Events;

interface EventDispatcher
{
    public function dispatch(object $event): object;
}

