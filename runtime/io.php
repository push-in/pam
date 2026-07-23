<?php

declare(strict_types=1);

namespace Pam\Net {
    use Pam\Async\NativeOperation;
    use Pam\Async\OperationKind;

    final class Dns
    {
        /** @return list<string> */
        public static function resolve(string $host, float $timeout = 5.0): array
        {
            if ($host === '' || strlen($host) > 253 || str_contains($host, "\0")) {
                throw new \InvalidArgumentException('DNS host must be a valid non-empty hostname.');
            }
            if (!NativeOperation::available()) {
                $addresses = gethostbynamel($host);
                if ($addresses === false) {
                    throw new \RuntimeException("DNS resolution failed for {$host}.");
                }
                return array_values(array_unique($addresses));
            }
            $result = NativeOperation::execute(OperationKind::Dns, ['host' => $host], $timeout);
            if (!is_array($result) || !is_array($result['addresses'] ?? null)) {
                throw new \RuntimeException('The native DNS resolver returned an invalid result.');
            }
            return array_values(array_filter($result['addresses'], is_string(...)));
        }
    }
}

namespace Pam\Filesystem {
    use Pam\Async\NativeOperation;
    use Pam\Async\OperationKind;

    final class File
    {
        public static function read(
            string $path,
            int $maxBytes = 16 * 1024 * 1024,
            float $timeout = 30.0,
        ): string {
            self::validate($path, $maxBytes);
            if (!NativeOperation::available()) {
                $readLimit = max(1, min(256 * 1024 * 1024 + 1, $maxBytes + 1));
                $contents = file_get_contents($path, false, null, 0, $readLimit);
                if (!is_string($contents)) {
                    throw new \RuntimeException("Unable to read {$path}.");
                }
                if (strlen($contents) > $maxBytes) {
                    throw new \RuntimeException("File {$path} exceeds the configured byte limit.");
                }
                return $contents;
            }
            $result = NativeOperation::execute(OperationKind::FileRead, [
                'path' => $path,
                'maxBytes' => $maxBytes,
            ], $timeout);
            $encoded = is_array($result) ? ($result['data'] ?? null) : null;
            if (!is_string($encoded) || ($contents = base64_decode($encoded, true)) === false) {
                throw new \RuntimeException('The native file reader returned invalid data.');
            }
            return $contents;
        }

        public static function write(
            string $path,
            string $contents,
            bool $atomic = true,
            float $timeout = 30.0,
        ): int {
            self::validate($path, max(1, strlen($contents)));
            if (!NativeOperation::available()) {
                $written = file_put_contents($path, $contents, LOCK_EX);
                if (!is_int($written)) {
                    throw new \RuntimeException("Unable to write {$path}.");
                }
                return $written;
            }
            $result = NativeOperation::execute(OperationKind::FileWrite, [
                'path' => $path,
                'data' => base64_encode($contents),
                'atomic' => $atomic,
            ], $timeout);
            $written = is_array($result) ? ($result['bytesWritten'] ?? null) : null;
            if (!is_int($written)) {
                throw new \RuntimeException('The native file writer returned an invalid result.');
            }
            return $written;
        }

        private static function validate(string $path, int $maxBytes): void
        {
            if ($path === '' || str_contains($path, "\0")) {
                throw new \InvalidArgumentException('File path must be non-empty and contain no NUL bytes.');
            }
            if ($maxBytes < 1 || $maxBytes > 256 * 1024 * 1024) {
                throw new \InvalidArgumentException('File byte limit must be between 1 and 256 MiB.');
            }
        }
    }
}

namespace Pam\Process {
    use Pam\Async\NativeOperation;
    use Pam\Async\OperationKind;

    enum CommandExitKind: int
    {
        case Exited = 1;
        case TimedOut = 2;
        case Signalled = 3;
    }

    final readonly class CommandResult
    {
        public function __construct(
            public CommandExitKind $kind,
            public int $exitCode,
            public string $stdout,
            public string $stderr,
        ) {
        }

        public function successful(): bool
        {
            return $this->kind === CommandExitKind::Exited && $this->exitCode === 0;
        }
    }

    final class Command
    {
        /** @param list<string> $arguments */
        public static function run(
            array $arguments,
            string $stdin = '',
            float $timeout = 30.0,
            int $maxOutputBytes = 8 * 1024 * 1024,
        ): CommandResult {
            if ($arguments === [] || $timeout <= 0 || $maxOutputBytes < 1) {
                throw new \InvalidArgumentException('Command, positive timeout, and output limit are required.');
            }
            foreach ($arguments as $argument) {
                if (str_contains($argument, "\0")) {
                    throw new \InvalidArgumentException('Command arguments must be strings without NUL bytes.');
                }
            }
            if (!NativeOperation::available()) {
                $result = (new \Pam\Task\ProcessPool(maxOutputBytes: $maxOutputBytes))
                    ->run($arguments, $stdin, $timeout);
                return new CommandResult(
                    CommandExitKind::from($result->kind->value),
                    $result->exitCode,
                    $result->stdout,
                    $result->stderr,
                );
            }
            $result = NativeOperation::execute(OperationKind::Process, [
                'arguments' => $arguments,
                'stdin' => base64_encode($stdin),
                'maxOutputBytes' => $maxOutputBytes,
            ], $timeout);
            if (!is_array($result)) {
                throw new \RuntimeException('The native process runner returned an invalid result.');
            }
            $stdout = base64_decode(is_string($result['stdout'] ?? null) ? $result['stdout'] : '', true);
            $stderr = base64_decode(is_string($result['stderr'] ?? null) ? $result['stderr'] : '', true);
            if ($stdout === false || $stderr === false) {
                throw new \RuntimeException('The native process runner returned invalid output.');
            }
            return new CommandResult(
                CommandExitKind::from(is_int($result['kind'] ?? null) ? $result['kind'] : 1),
                is_int($result['exitCode'] ?? null) ? $result['exitCode'] : -1,
                $stdout,
                $stderr,
            );
        }
    }
}

namespace Pam\Signal {
    use Pam\Async\NativeOperation;
    use Pam\Async\OperationKind;

    final class Signal
    {
        public static function wait(int $signal, float $timeout = 30.0): int
        {
            if ($signal < 1 || $signal > 64) {
                throw new \InvalidArgumentException('Signal must be between 1 and 64.');
            }
            if (!NativeOperation::available()) {
                if (!function_exists('pcntl_signal')) {
                    throw new \LogicException('Signal waiting requires Pam native I/O or pcntl.');
                }
                $received = false;
                pcntl_async_signals(true);
                pcntl_signal($signal, static function () use (&$received): void { $received = true; });
                $deadline = microtime(true) + $timeout;
                while (!$received && microtime(true) < $deadline) {
                    \Pam\Async\delay(0.001);
                }
                if (!$received) {
                    throw new \RuntimeException('Signal wait timed out.');
                }
                return $signal;
            }
            $result = NativeOperation::execute(OperationKind::Signal, ['signal' => $signal], $timeout);
            if (!is_array($result) || !is_int($result['signal'] ?? null)) {
                throw new \RuntimeException('The native signal watcher returned an invalid result.');
            }
            return $result['signal'];
        }
    }
}
