<?php

declare(strict_types=1);

namespace Pam\Laravel\Enums;

enum SpanKind: int
{
    case Internal = 1;
    case Server = 2;
    case Client = 3;
    case Producer = 4;
    case Consumer = 5;
}
