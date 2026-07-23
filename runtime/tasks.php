<?php

declare(strict_types=1);

namespace Pam\Task {
    use Pam\Async\Future;
    use function Pam\Async\delay;

    enum ProcessExitKind: int
    {
        case Exited = 1;
        case TimedOut = 2;
        case Signalled = 3;
    }

    final readonly class ProcessResult
    {
        public function __construct(
            public ProcessExitKind $kind,
            public int $exitCode,
            public string $stdout,
            public string $stderr,
            public float $duration,
            public bool $stdoutTruncated = false,
            public bool $stderrTruncated = false,
        ) {
        }

        public function successful(): bool
        {
            return $this->kind === ProcessExitKind::Exited && $this->exitCode === 0;
        }
    }

    final class ProcessPool
    {
        private int $activeProcesses = 0;

        public function __construct(
            private readonly int $maxWorkers = 4,
            private readonly int $maxOutputBytes = 8 * 1024 * 1024,
            private readonly float $terminationGrace = 0.25,
        ) {
            if ($maxWorkers < 1 || $maxWorkers > 256) {
                throw new \InvalidArgumentException('ProcessPool maxWorkers must be between 1 and 256.');
            }
            if ($maxOutputBytes < 1) {
                throw new \InvalidArgumentException('ProcessPool maxOutputBytes must be positive.');
            }
            if ($terminationGrace < 0) {
                throw new \InvalidArgumentException('ProcessPool terminationGrace cannot be negative.');
            }
        }

        /** @param list<string> $command */
        public function submit(array $command, string $stdin = '', float $timeout = 30.0): Future
        {
            return new Future(fn (): ProcessResult => $this->run($command, $stdin, $timeout));
        }

        /** @param list<string> $command */
        public function run(array $command, string $stdin = '', float $timeout = 30.0): ProcessResult
        {
            if ($command === [] || $timeout <= 0) {
                throw new \InvalidArgumentException('A command and positive timeout are required.');
            }
            foreach ($command as $argument) {
                if (str_contains($argument, "\0")) {
                    throw new \InvalidArgumentException('Process arguments must be strings without NUL bytes.');
                }
            }

            $started = microtime(true);
            $deadline = $started + $timeout;
            while ($this->activeProcesses >= $this->maxWorkers) {
                if (microtime(true) >= $deadline) {
                    return new ProcessResult(
                        ProcessExitKind::TimedOut,
                        -1,
                        '',
                        'Process timed out while waiting for an execution slot.',
                        microtime(true) - $started,
                    );
                }
                delay(0.001);
            }
            ++$this->activeProcesses;

            try {
                return $this->execute($command, $stdin, $started, $deadline);
            } finally {
                --$this->activeProcesses;
            }
        }

        /** @param list<string> $command */
        private function execute(array $command, string $stdin, float $started, float $deadline): ProcessResult
        {
            $pipes = [];
            $process = proc_open(
                $command,
                [
                    0 => ['pipe', 'r'],
                    1 => ['pipe', 'w'],
                    2 => ['pipe', 'w'],
                ],
                $pipes,
                null,
                null,
                ['bypass_shell' => true],
            );
            if (!is_resource($process)) {
                throw new \RuntimeException('Unable to start isolated process.');
            }
            foreach ($pipes as $pipe) {
                stream_set_blocking($pipe, false);
            }
            $stdout = '';
            $stderr = '';
            $stdoutTruncated = false;
            $stderrTruncated = false;
            $kind = ProcessExitKind::Exited;
            $knownExitCode = -1;
            $stdinOffset = 0;
            $pid = null;

            $initialStatus = proc_get_status($process);
            $pid = $initialStatus['pid'];
            if ($pid > 0 && function_exists('posix_setpgid')) {
                @posix_setpgid($pid, $pid);
            }

            try {
                while (true) {
                    $status = proc_get_status($process);
                    $this->drain($pipes[1], $stdout, $stdoutTruncated);
                    $this->drain($pipes[2], $stderr, $stderrTruncated);

                    if (is_resource($pipes[0])) {
                        if ($stdinOffset >= strlen($stdin)) {
                            fclose($pipes[0]);
                        } else {
                            $written = @fwrite($pipes[0], substr($stdin, $stdinOffset, 16 * 1024));
                            if (is_int($written) && $written > 0) {
                                $stdinOffset += $written;
                            }
                        }
                    }

                    if (!$status['running']) {
                        $knownExitCode = (int) $status['exitcode'];
                        if ($status['signaled']) {
                            $kind = ProcessExitKind::Signalled;
                        }
                        break;
                    }
                    if (microtime(true) >= $deadline) {
                        $kind = ProcessExitKind::TimedOut;
                        $this->terminate($process, $pid, 15);
                        $graceDeadline = microtime(true) + $this->terminationGrace;
                        do {
                            delay(0.001);
                            $status = proc_get_status($process);
                            $this->drain($pipes[1], $stdout, $stdoutTruncated);
                            $this->drain($pipes[2], $stderr, $stderrTruncated);
                        } while ($status['running'] && microtime(true) < $graceDeadline);
                        if ($status['running']) {
                            $this->terminate($process, $pid, 9);
                        }
                        break;
                    }
                    delay(0.001);
                }
            } finally {
                if (is_resource($pipes[0])) {
                    fclose($pipes[0]);
                }
                $this->drain($pipes[1], $stdout, $stdoutTruncated);
                $this->drain($pipes[2], $stderr, $stderrTruncated);
                fclose($pipes[1]);
                fclose($pipes[2]);
            }

            $exitCode = proc_close($process);
            if ($exitCode === -1 && $knownExitCode >= 0) {
                $exitCode = $knownExitCode;
            }
            return new ProcessResult(
                $kind,
                $exitCode,
                $stdout,
                $stderr,
                microtime(true) - $started,
                $stdoutTruncated,
                $stderrTruncated,
            );
        }

        /** @param resource $stream */
        private function drain($stream, string &$output, bool &$truncated): void
        {
            for ($read = 0; $read < 64; ++$read) {
                $chunk = @fread($stream, 8192);
                if (!is_string($chunk) || $chunk === '') {
                    return;
                }
                $remaining = $this->maxOutputBytes - strlen($output);
                if ($remaining > 0) {
                    $output .= substr($chunk, 0, $remaining);
                }
                if (strlen($chunk) > $remaining) {
                    $truncated = true;
                }
            }
        }

        /** @param resource $process */
        private function terminate($process, ?int $pid, int $signal): void
        {
            if ($pid !== null && $pid > 0 && function_exists('posix_kill')) {
                if (@posix_kill(-$pid, $signal)) {
                    return;
                }
                @posix_kill($pid, $signal);
            }
            @proc_terminate($process, $signal);
        }

        /** @param list<string> $arguments */
        public function php(string $script, array $arguments = [], float $timeout = 30.0): ProcessResult
        {
            $binary = is_file(PHP_BINARY) ? PHP_BINARY : PHP_BINDIR . DIRECTORY_SEPARATOR . 'php';
            return $this->run([$binary, $script, ...$arguments], timeout: $timeout);
        }
    }
}
