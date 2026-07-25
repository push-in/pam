<?php

declare(strict_types=1);

namespace Pam\Laravel\Enums;

enum StackPreset: int
{
    case Api = 1;
    case Livewire = 2;
    case Inertia = 3;
    case Realtime = 4;

    public static function fromName(string $name): ?self
    {
        return match ($name) {
            'api' => self::Api,
            'livewire' => self::Livewire,
            'inertia' => self::Inertia,
            'realtime' => self::Realtime,
            default => null,
        };
    }
}
