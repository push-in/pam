<?php

declare(strict_types=1);

namespace Nimbus\Http;

/** An outgoing cookie, rendered as one `Set-Cookie` header by `toHeader()`. */
final readonly class Cookie
{
    /**
     * @param int $maxAge Seconds until it expires; 0 is a browser-session cookie, negative deletes it.
     * @param string $sameSite Lax, Strict or None.
     */
    public function __construct(
        public string $name,
        public string $value,
        public int $maxAge = 0,
        public string $path = '/',
        public ?string $domain = null,
        public bool $secure = true,
        public bool $httpOnly = true,
        public string $sameSite = 'Lax',
    ) {
        if ($name === '' || preg_match('/[\x00-\x20\x7f()<>@,;:"\/\[\]?={}\\\\]/', $name) === 1) {
            throw new \InvalidArgumentException("Invalid cookie name \"{$name}\".");
        }
        if (!in_array($sameSite, ['Lax', 'Strict', 'None'], true)) {
            throw new \InvalidArgumentException('SameSite must be Lax, Strict or None.');
        }
        if ($sameSite === 'None' && !$secure) {
            throw new \InvalidArgumentException('SameSite=None cookies must be secure.');
        }
        if (preg_match('/[;\r\n]/', $path . ($domain ?? '')) === 1) {
            throw new \InvalidArgumentException('Cookie path and domain cannot contain ";" or line breaks.');
        }
    }

    /** A cookie that deletes `$name` in the browser. */
    public static function forget(string $name, string $path = '/', ?string $domain = null): self
    {
        return new self($name, '', -1, $path, $domain);
    }

    public function withValue(string $value): self
    {
        return new self($this->name, $value, $this->maxAge, $this->path, $this->domain, $this->secure, $this->httpOnly, $this->sameSite);
    }

    /** @param int|null $now Seconds since the epoch, for the `Expires` attribute. */
    public function toHeader(?int $now = null): string
    {
        $parts = [$this->name . '=' . rawurlencode($this->value)];
        if ($this->maxAge !== 0) {
            $maxAge = max(0, $this->maxAge);
            $parts[] = 'Expires=' . gmdate('D, d M Y H:i:s', ($now ?? time()) + ($this->maxAge < 0 ? -31_536_000 : $maxAge)) . ' GMT';
            $parts[] = 'Max-Age=' . $maxAge;
        }
        $parts[] = 'Path=' . $this->path;
        if ($this->domain !== null) {
            $parts[] = 'Domain=' . $this->domain;
        }
        if ($this->secure) {
            $parts[] = 'Secure';
        }
        if ($this->httpOnly) {
            $parts[] = 'HttpOnly';
        }
        $parts[] = 'SameSite=' . $this->sameSite;
        return implode('; ', $parts);
    }
}
