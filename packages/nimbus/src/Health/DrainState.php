<?php

declare(strict_types=1);

namespace Nimbus\Health;

/**
 * Whether this worker is draining: it finishes the requests and jobs it holds
 * and must receive no new traffic, so readiness answers 503.
 *
 * Under PAM the runtime reports it through `pam_draining()` from the moment a
 * worker receives SIGTERM, is replaced by a reload or reaches its request
 * limit. A drain file (`NIMBUS_DRAIN_FILE`) takes the instance out of the load
 * balancer by hand, for maintenance, without stopping it.
 */
final readonly class DrainState
{
    /** @var \Closure(): bool */
    private \Closure $runtime;

    /** @param (\Closure(): bool)|null $runtime Defaults to PAM's `pam_draining()`. */
    public function __construct(private ?string $drainFile = null, ?\Closure $runtime = null)
    {
        $this->runtime = $runtime ?? static fn (): bool => \function_exists('pam_draining') && \pam_draining() === true;
    }

    public static function fromEnvironment(): self
    {
        $file = getenv('NIMBUS_DRAIN_FILE');
        return new self(is_string($file) && $file !== '' ? $file : null);
    }

    public function draining(): bool
    {
        return ($this->runtime)() || ($this->drainFile !== null && is_file($this->drainFile));
    }
}
