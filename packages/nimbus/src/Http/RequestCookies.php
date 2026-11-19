<?php

declare(strict_types=1);

namespace Nimbus\Http;

use Pam\Http\Request;

/**
 * The cookies of the current request. `EncryptCookiesMiddleware` registers the decrypted
 * values in the request scope; without it, `fromRequest()` parses the raw `Cookie` header.
 */
final readonly class RequestCookies
{
    /** @param array<string, string> $cookies */
    public function __construct(private array $cookies = [])
    {
    }

    public static function fromRequest(Request $request): self
    {
        return new self(self::parse($request->getHeader('cookie') ?? ''));
    }

    /**
     * The name/value pairs of a `Cookie` header; the first occurrence of a name wins.
     *
     * @return array<string, string>
     */
    public static function parse(string $header): array
    {
        $cookies = [];
        foreach (explode(';', $header) as $pair) {
            $separator = strpos($pair, '=');
            if ($separator === false) {
                continue;
            }
            $name = trim(substr($pair, 0, $separator));
            $value = trim(substr($pair, $separator + 1));
            if ($name === '' || array_key_exists($name, $cookies)) {
                continue;
            }
            if (strlen($value) >= 2 && $value[0] === '"' && $value[-1] === '"') {
                $value = substr($value, 1, -1);
            }
            $cookies[$name] = rawurldecode($value);
        }
        return $cookies;
    }

    public function get(string $name, ?string $default = null): ?string
    {
        return $this->cookies[$name] ?? $default;
    }

    public function has(string $name): bool
    {
        return array_key_exists($name, $this->cookies);
    }

    /** @return array<string, string> */
    public function all(): array
    {
        return $this->cookies;
    }
}
