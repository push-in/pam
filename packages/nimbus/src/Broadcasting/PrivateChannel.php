<?php

declare(strict_types=1);

namespace Nimbus\Broadcasting;

/** A channel whose subscribers pass `BroadcastManager::channel()` authorization (`private-` prefix). */
class PrivateChannel extends Channel
{
    public function __construct(string $name)
    {
        parent::__construct(str_starts_with($name, 'private-') ? $name : 'private-' . $name);
    }
}
