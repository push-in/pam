<?php

declare(strict_types=1);

namespace Pam\Laravel\Services;

use Illuminate\Http\Request;
use Pam\Laravel\Contracts\LifecycleHook;
use Symfony\Component\HttpFoundation\Response;

final class LifecycleManager
{
    /** @var list<LifecycleHook> */
    private array $hooks = [];

    public function register(LifecycleHook $hook): void
    {
        $this->hooks[] = $hook;
    }

    public function before(Request $request): void
    {
        foreach ($this->hooks as $hook) {
            $hook->beforeRequest($request);
        }
    }

    public function after(Request $request, Response $response, int $durationNanoseconds): void
    {
        foreach (array_reverse($this->hooks) as $hook) {
            $hook->afterRequest($request, $response, $durationNanoseconds);
        }
    }
}
