<?php

declare(strict_types=1);

namespace Pam\Api\Http;

use Pam\Http\Request;

final readonly class ClientIpResolver
{
    /** @param list<string> $trustedProxies */
    public function __construct(private array $trustedProxies = [])
    {
    }

    public function resolve(Request $request): string
    {
        $remote = is_string($_SERVER['REMOTE_ADDR'] ?? null) ? $_SERVER['REMOTE_ADDR'] : 'unknown';
        if (!in_array($remote, $this->trustedProxies, true)) {
            return $remote;
        }
        $forwarded = $request->getHeader('x-forwarded-for');
        if ($forwarded === null) {
            return $remote;
        }
        $candidate = trim(explode(',', $forwarded)[0]);
        return filter_var($candidate, FILTER_VALIDATE_IP) === false ? $remote : $candidate;
    }
}

