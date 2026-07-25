<?php

declare(strict_types=1);

namespace Pam\Laravel\Enums;

enum PackageCategory: int
{
    case LaravelFirstParty = 1;
    case ApplicationUi = 2;
    case DomainTooling = 3;
    case DataMediaInfrastructure = 4;
    case Observability = 5;
    case AuthenticationMultitenancy = 6;
    case ApiBackoffice = 7;
    case DevelopmentQuality = 8;
}
