<?php

declare(strict_types=1);

namespace Pam\Contracts\Runtime;

enum Stability: int
{
    case Experimental = 1;
    case Stable = 2;
    case Deprecated = 3;
}
