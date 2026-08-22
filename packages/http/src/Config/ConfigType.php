<?php

declare(strict_types=1);

namespace Pam\Api\Config;

enum ConfigType: int
{
    case String = 1;
    case Integer = 2;
    case Boolean = 3;
    case Float = 4;
}
