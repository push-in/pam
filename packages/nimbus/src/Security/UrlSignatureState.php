<?php

declare(strict_types=1);

namespace Nimbus\Security;

enum UrlSignatureState: int
{
    case Valid = 1;
    case Invalid = 2;
    case Expired = 3;
}
