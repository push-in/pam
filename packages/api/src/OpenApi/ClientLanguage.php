<?php

declare(strict_types=1);

namespace Pam\Api\OpenApi;

enum ClientLanguage: int
{
    case TypeScript = 1;
    case Kotlin = 2;
    case Swift = 3;
}

