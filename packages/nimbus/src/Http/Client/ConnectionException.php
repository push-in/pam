<?php

declare(strict_types=1);

namespace Nimbus\Http\Client;

/** No response was received: DNS, connect, TLS or timeout failure. */
class ConnectionException extends \RuntimeException
{
}
