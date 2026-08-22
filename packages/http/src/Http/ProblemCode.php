<?php

declare(strict_types=1);

namespace Pam\Api\Http;

enum ProblemCode: int
{
    case ValidationFailed = 1;
    case Unauthenticated = 2;
    case Forbidden = 3;
    case NotFound = 4;
    case Conflict = 5;
    case RateLimited = 6;
    case Timeout = 7;
    case InternalError = 8;
    case MethodNotAllowed = 9;
}
