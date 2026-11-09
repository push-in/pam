<?php

namespace App\Jobs;

use Illuminate\Contracts\Queue\ShouldQueue;
use Illuminate\Foundation\Queue\Queueable;
use Illuminate\Support\Facades\Cache;

class ReadinessJob implements ShouldQueue
{
    use Queueable;

    public function __construct(public string $token)
    {
    }

    public function handle(): void
    {
        Cache::put('readiness-job:'.$this->token, 'done:'.getmypid(), 300);
    }
}
