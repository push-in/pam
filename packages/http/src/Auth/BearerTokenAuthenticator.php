<?php

declare(strict_types=1);

namespace Pam\Api\Auth;

use Pam\Http\Request;

final readonly class BearerTokenAuthenticator implements Authenticator
{
    private \Closure $clock;

    public function __construct(
        private HmacTokenCodec $tokens,
        private ?TokenRevocationStore $revocations = null,
        ?callable $clock = null,
    ) {
        $this->clock = $clock === null ? static fn (): int => time() : \Closure::fromCallable($clock);
    }

    public function authenticate(Request $request): ?Principal
    {
        $authorization = $request->getHeader('authorization');
        if ($authorization === null || preg_match('/^Bearer ([A-Za-z0-9._-]+)$/D', $authorization, $matches) !== 1) {
            return null;
        }
        $principal = $this->tokens->verify($matches[1]);
        if ($principal === null || ($this->revocations !== null
            && $this->revocations->isRevoked($principal->tokenIdentifier, ($this->clock)())
        )) {
            return null;
        }
        return $principal;
    }
}
