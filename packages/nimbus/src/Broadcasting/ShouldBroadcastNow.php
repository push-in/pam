<?php

declare(strict_types=1);

namespace Nimbus\Broadcasting;

/** A broadcast event sent during `dispatch()`, never queued. */
interface ShouldBroadcastNow extends ShouldBroadcast
{
}
