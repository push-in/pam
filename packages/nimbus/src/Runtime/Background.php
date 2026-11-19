<?php

declare(strict_types=1);

namespace Nimbus\Runtime;

use Nimbus\Jobs\JobWorker;
use Nimbus\Scheduling\Scheduler;
use Pam\Http\Request;
use Pam\Http\Response;

/**
 * Work that runs beside HTTP traffic: queue workers and the scheduler.
 *
 * Inside PAM it rides the task workers (`pam start --task-workers N`): the
 * master sends one authenticated tick per second to the pool, never two at
 * once, and App::handle() answers it by calling tick(), so no system cron or
 * extra process is needed. Outside a cluster, run() is a standalone loop for a
 * dedicated process (`php bin/worker.php`) that sleeps through Pam\Async when
 * the runtime is present.
 */
final class Background
{
    private const string KIND_HEADER = 'x-pam-task';
    private const string TOKEN_HEADER = 'x-pam-task-token';

    /** @var array<string, \Closure(int): mixed> */
    private array $tasks = [];

    private bool $stopping = false;

    /** @var \Closure(): int */
    private readonly \Closure $clock;

    /** @var \Closure(string, \Throwable): void */
    private readonly \Closure $onFailure;

    /**
     * @param (\Closure(): int)|null $clock Seconds since the epoch; defaults to time().
     * @param (\Closure(string, \Throwable): void)|null $onFailure Defaults to the runtime log.
     */
    public function __construct(?\Closure $clock = null, ?\Closure $onFailure = null)
    {
        $this->clock = $clock ?? time(...);
        $this->onFailure = $onFailure ?? static function (string $task, \Throwable $error): void {
            if (class_exists(\Pam\Observability\Telemetry::class)) {
                \Pam\Observability\Telemetry::log('error', 'Background task failed', [
                    'task' => $task,
                    'exception' => $error::class,
                ]);
            }
        };
    }

    /** @param callable(int): mixed $task Receives the tick time in seconds since the epoch. */
    public function add(string $name, callable $task): self
    {
        if ($name === '' || isset($this->tasks[$name])) {
            throw new \InvalidArgumentException("Background task names must be unique and non-empty: {$name}.");
        }
        $this->tasks[$name] = \Closure::fromCallable($task);
        return $this;
    }

    public function scheduler(Scheduler $scheduler, string $name = 'scheduler'): self
    {
        return $this->add($name, static fn (int $now): array => $scheduler->runDue($now));
    }

    /** Drains up to `$maximumJobs` jobs or `$maximumSeconds` per tick. */
    public function queue(
        JobWorker $worker,
        string $name = 'queue',
        int $maximumJobs = 100,
        float $maximumSeconds = 0.8,
    ): self {
        $clock = $this->clock;
        return $this->add(
            $name,
            static fn (int $now): int => $worker->work($maximumJobs, $maximumSeconds, $clock),
        );
    }

    /** Runs every task once; a failing task is reported and does not stop the others. */
    public function tick(): void
    {
        foreach ($this->tasks as $name => $task) {
            try {
                $task(($this->clock)());
            } catch (\Throwable $error) {
                ($this->onFailure)($name, $error);
            }
        }
    }

    /**
     * Ticks every `$intervalSeconds` until stop(), SIGTERM or SIGINT (with
     * ext-pcntl), or `$maximumTicks`.
     */
    public function run(float $intervalSeconds = 1.0, ?int $maximumTicks = null): void
    {
        if ($intervalSeconds <= 0.0 || ($maximumTicks !== null && $maximumTicks < 1)) {
            throw new \InvalidArgumentException('The interval and the tick limit must be positive.');
        }
        $this->stopping = false;
        if (function_exists('pcntl_async_signals') && function_exists('pcntl_signal')) {
            pcntl_async_signals(true);
            pcntl_signal(SIGTERM, $this->stop(...));
            pcntl_signal(SIGINT, $this->stop(...));
        }
        for ($ticks = 0; !$this->stopping && ($maximumTicks === null || $ticks < $maximumTicks); ++$ticks) {
            $started = microtime(true);
            $this->tick();
            $remaining = $intervalSeconds - (microtime(true) - $started);
            if ($this->stopping || ($maximumTicks !== null && $ticks + 1 >= $maximumTicks) || $remaining <= 0.0) {
                continue;
            }
            if (function_exists('Pam\\Async\\delay')) {
                \Pam\Async\delay($remaining);
            } else {
                usleep((int) ($remaining * 1_000_000));
            }
        }
    }

    public function stop(): void
    {
        $this->stopping = true;
    }

    /**
     * Answers a request of the PAM task-worker protocol, or returns null for
     * every other request. Only a task worker with the cluster's token accepts
     * it, so public traffic cannot trigger a tick.
     */
    public function answer(Request $request, Response $response): ?Response
    {
        $kind = $request->getHeader(self::KIND_HEADER);
        if ($kind === null || getenv('PAM_TASK_WORKER') !== '1') {
            return null;
        }
        $token = getenv('PAM_TASK_TOKEN');
        $given = $request->getHeader(self::TOKEN_HEADER);
        if (!is_string($token) || strlen($token) < 32 || $given === null || !hash_equals($token, $given)) {
            return $response->status(403)->send('');
        }
        if ($kind !== 'tick') {
            // Task payloads belong to other frameworks' task protocols.
            return $response->status(404)->send('');
        }
        $this->tick();
        return $response->status(204)->send('');
    }
}
