<?php

declare(strict_types=1);

namespace Nimbus\Http;

use Pam\Http\Request;

/**
 * The client address of a request, behind trusted proxies.
 *
 * A proxy appends the address it received the connection from to `X-Forwarded-For`, so only
 * the right end of the header is trustworthy: the client is the first address, walking from the
 * right, that is not a trusted proxy. Entries that are not IP addresses stop the walk at the last
 * trusted hop. Under PAM the runtime already resolves `REMOTE_ADDR` this way when its
 * `trustedProxies` option (or `PAM_TRUSTED_PROXIES`) is set.
 */
final readonly class ClientIpResolver
{
    private const MAX_HOPS = 32;

    /** @var list<array{string, int}> packed network address and prefix length */
    private array $networks;

    /** @param list<string> $trustedProxies addresses or CIDR ranges (`10.0.0.0/8`, `fd00::/8`) */
    public function __construct(array $trustedProxies = [])
    {
        $networks = [];
        foreach ($trustedProxies as $proxy) {
            $networks[] = self::network($proxy);
        }
        $this->networks = $networks;
    }

    public function resolve(Request $request): string
    {
        $client = is_string($_SERVER['REMOTE_ADDR'] ?? null) ? $_SERVER['REMOTE_ADDR'] : 'unknown';
        if ($this->networks === [] || !$this->trusts($client)) {
            return $client;
        }
        $forwarded = $request->getHeader('x-forwarded-for');
        if ($forwarded === null) {
            return $client;
        }
        $entries = array_reverse(explode(',', $forwarded));
        foreach (array_slice($entries, 0, self::MAX_HOPS) as $entry) {
            $entry = trim($entry);
            if (filter_var($entry, FILTER_VALIDATE_IP) === false) {
                return $client;
            }
            $client = $entry;
            if (!$this->trusts($client)) {
                return $client;
            }
        }
        return $client;
    }

    /** Whether the address is one of the trusted proxies. */
    public function trusts(string $address): bool
    {
        $packed = @inet_pton($address);
        if ($packed === false) {
            return false;
        }
        foreach ($this->networks as [$network, $prefix]) {
            if (strlen($network) === strlen($packed) && self::matches($packed, $network, $prefix)) {
                return true;
            }
        }
        return false;
    }

    private static function matches(string $address, string $network, int $prefix): bool
    {
        $bytes = intdiv($prefix, 8);
        if (strncmp($address, $network, $bytes) !== 0) {
            return false;
        }
        $bits = $prefix % 8;
        if ($bits === 0) {
            return true;
        }
        $mask = (0xff << (8 - $bits)) & 0xff;
        return (ord($address[$bytes]) & $mask) === (ord($network[$bytes]) & $mask);
    }

    /** @return array{string, int} */
    private static function network(string $proxy): array
    {
        $parts = explode('/', trim($proxy), 2);
        $packed = @inet_pton($parts[0]);
        $width = $packed === false ? 0 : strlen($packed) * 8;
        $prefix = isset($parts[1]) && ctype_digit($parts[1]) ? (int) $parts[1] : (isset($parts[1]) ? -1 : $width);
        if ($packed === false || $prefix < 0 || $prefix > $width) {
            throw new \InvalidArgumentException(sprintf('Trusted proxy "%s" is not an IP address or CIDR range.', $proxy));
        }
        return [$packed, $prefix];
    }
}
