<?php

declare(strict_types=1);

namespace Pam\Laravel\Enums;

enum RemoteProviderType: int
{
    case PamCloud = 1;
    case Forge = 2;

    public static function fromName(string $name): ?self
    {
        return match (strtolower($name)) {
            'cloud', 'pam-cloud' => self::PamCloud,
            'forge' => self::Forge,
            default => null,
        };
    }
}
