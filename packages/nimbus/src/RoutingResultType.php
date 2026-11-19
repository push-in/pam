<?php

declare(strict_types=1);

namespace Nimbus;

enum RoutingResultType: int
{
    case Found = 1;
    case MethodNotAllowed = 2;
    case NotFound = 3;
}
