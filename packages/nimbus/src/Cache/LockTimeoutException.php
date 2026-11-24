<?php

declare(strict_types=1);

namespace Nimbus\Cache;

/** `Lock::block()` did not get the lock in time. */
final class LockTimeoutException extends \RuntimeException
{
}
