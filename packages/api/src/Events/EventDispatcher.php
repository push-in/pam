<?php

declare(strict_types=1);

namespace Pam\Api\Events;

interface EventDispatcher
{
    public function dispatch(object $event): object;
}

