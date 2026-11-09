<?php

namespace App\Events;

use Illuminate\Broadcasting\Channel;
use Illuminate\Contracts\Broadcasting\ShouldBroadcastNow;

class ReadinessPing implements ShouldBroadcastNow
{
    public function __construct(public string $message)
    {
    }

    public function broadcastOn(): Channel
    {
        return new Channel('readiness');
    }

    public function broadcastAs(): string
    {
        return 'readiness.ping';
    }
}
