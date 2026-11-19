<?php

declare(strict_types=1);

namespace Nimbus\Session;

/** Session cookie and lifetime settings. */
final readonly class SessionConfig
{
    /**
     * @param int $lifetime Idle seconds before the session expires.
     * @param string $sameSite Lax, Strict or None.
     * @param array{int, int} $lottery Chance (`[2, 100]` = 2%) that a request prunes expired sessions.
     */
    public function __construct(
        public string $cookie = 'nimbus_session',
        public int $lifetime = 7_200,
        public bool $expireOnClose = false,
        public string $path = '/',
        public ?string $domain = null,
        public bool $secure = true,
        public string $sameSite = 'Lax',
        public array $lottery = [2, 100],
    ) {
        if ($lifetime < 1) {
            throw new \InvalidArgumentException('The session lifetime must be positive.');
        }
        if ($lottery[0] < 0 || $lottery[1] < 1) {
            throw new \InvalidArgumentException('The session lottery must be [chances, out of].');
        }
    }
}
