<?php

declare(strict_types=1);

namespace Pam\Native {
    enum Capability: int
    {
        case Timer = 1;
        case StreamReadiness = 2;
        case Dns = 3;
        case Filesystem = 4;
        case Process = 5;
        case Signal = 6;
        case HttpStreaming = 7;
        case WebSocket = 8;
        case Http3 = 9;
    }

    final class Api
    {
        public const ABI_VERSION = 1;

        public static function abiVersion(): int
        {
            if (!function_exists('pam_native_abi_version')) {
                return 0;
            }
            return pam_native_abi_version();
        }

        /** @return list<Capability> */
        public static function capabilities(): array
        {
            if (self::abiVersion() !== self::ABI_VERSION) {
                return [];
            }
            return Capability::cases();
        }

        public static function supports(Capability $capability): bool
        {
            return in_array($capability, self::capabilities(), true);
        }
    }
}
