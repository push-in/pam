<?php

declare(strict_types=1);

namespace Pam\Api\Container;

enum BindingLifetime: int
{
    case Transient = 1;
    case Singleton = 2;
    case Scoped = 3;
}

