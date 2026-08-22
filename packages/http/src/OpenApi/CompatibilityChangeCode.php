<?php

declare(strict_types=1);

namespace Pam\Api\OpenApi;

enum CompatibilityChangeCode: int
{
    case PathRemoved = 1;
    case OperationRemoved = 2;
    case RequiredInputAdded = 3;
}

