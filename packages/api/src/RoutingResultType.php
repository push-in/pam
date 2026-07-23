<?php

declare(strict_types=1);

namespace Pam\Api;

enum RoutingResultType: int
{
    case Found = 1;
    case MethodNotAllowed = 2;
    case NotFound = 3;
}
