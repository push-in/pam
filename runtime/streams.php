<?php

declare(strict_types=1);

namespace Pam\Stream {
    use Pam\Async\CancellationToken;

    interface ReadableStream
    {
        public function read(?int $length = null, ?float $timeout = null): string;
        public function eof(): bool;
        public function close(): void;
    }

    interface WritableStream
    {
        public function write(string $data, ?float $timeout = null): int;
        public function close(): void;
    }

    interface DuplexStream extends ReadableStream, WritableStream
    {
    }

    class Readable implements ReadableStream
    {
        /** @var resource|null */
        protected $resource;

        /** @param resource $resource */
        public function __construct(
            $resource,
            protected readonly int $highWaterMark = 64 * 1024,
            protected readonly ?CancellationToken $cancellation = null,
        ) {
            if (!is_resource($resource)) {
                throw new \InvalidArgumentException('Readable requires a stream resource.');
            }
            if ($highWaterMark < 1 || $highWaterMark > 16 * 1024 * 1024) {
                throw new \InvalidArgumentException('Stream highWaterMark must be between 1 byte and 16 MiB.');
            }
            $this->resource = $resource;
            stream_set_blocking($this->resource, false);
        }

        public function read(?int $length = null, ?float $timeout = null): string
        {
            $resource = $this->resource();
            $length ??= $this->highWaterMark;
            if ($length < 1 || $length > $this->highWaterMark) {
                throw new \InvalidArgumentException('Read length must fit within the stream highWaterMark.');
            }
            return \Pam\Async\read($resource, $length, $timeout, $this->cancellation);
        }

        /** @return \Generator<int, string> */
        public function chunks(?float $timeout = null): \Generator
        {
            while (!$this->eof()) {
                $chunk = $this->read(timeout: $timeout);
                if ($chunk !== '') {
                    yield $chunk;
                }
            }
        }

        public function eof(): bool
        {
            return !is_resource($this->resource) || feof($this->resource);
        }

        public function close(): void
        {
            if (is_resource($this->resource)) {
                fclose($this->resource);
            }
            $this->resource = null;
        }

        /** @return resource */
        public function resource()
        {
            $resource = $this->resource;
            if (!is_resource($resource)) {
                throw new \LogicException('Stream is closed.');
            }
            return $resource;
        }
    }

    class Writable implements WritableStream
    {
        /** @var resource|null */
        protected $resource;

        /** @param resource $resource */
        public function __construct(
            $resource,
            protected readonly int $highWaterMark = 64 * 1024,
            protected readonly ?CancellationToken $cancellation = null,
        ) {
            if (!is_resource($resource)) {
                throw new \InvalidArgumentException('Writable requires a stream resource.');
            }
            if ($highWaterMark < 1 || $highWaterMark > 16 * 1024 * 1024) {
                throw new \InvalidArgumentException('Stream highWaterMark must be between 1 byte and 16 MiB.');
            }
            $this->resource = $resource;
            stream_set_blocking($this->resource, false);
        }

        public function write(string $data, ?float $timeout = null): int
        {
            $resource = $this->resource();
            $written = 0;
            $deadline = $timeout === null ? null : microtime(true) + $timeout;
            while ($written < strlen($data)) {
                $chunk = substr($data, $written, $this->highWaterMark);
                $remaining = $deadline === null ? null : max(0.0, $deadline - microtime(true));
                \Pam\Async\write($resource, $chunk, $remaining, $this->cancellation);
                $written += strlen($chunk);
            }
            return $written;
        }

        public function close(): void
        {
            if (is_resource($this->resource)) {
                fclose($this->resource);
            }
            $this->resource = null;
        }

        /** @return resource */
        public function resource()
        {
            $resource = $this->resource;
            if (!is_resource($resource)) {
                throw new \LogicException('Stream is closed.');
            }
            return $resource;
        }
    }

    final class Duplex implements DuplexStream
    {
        private readonly Readable $readable;
        private readonly Writable $writable;

        /** @param resource $resource */
        public function __construct($resource, int $highWaterMark = 64 * 1024)
        {
            $this->readable = new Readable($resource, $highWaterMark);
            $this->writable = new Writable($resource, $highWaterMark);
        }

        public function read(?int $length = null, ?float $timeout = null): string
        {
            return $this->readable->read($length, $timeout);
        }

        public function write(string $data, ?float $timeout = null): int
        {
            return $this->writable->write($data, $timeout);
        }

        public function eof(): bool { return $this->readable->eof(); }
        public function close(): void { $this->readable->close(); }
    }

    final class Streams
    {
        /** @param resource $resource */
        public static function readable($resource, int $highWaterMark = 64 * 1024): Readable
        {
            return new Readable($resource, $highWaterMark);
        }

        /** @param resource $resource */
        public static function writable($resource, int $highWaterMark = 64 * 1024): Writable
        {
            return new Writable($resource, $highWaterMark);
        }

        public static function connect(
            string $address,
            float $timeout = 10.0,
            bool $tls = false,
            int $highWaterMark = 64 * 1024,
        ): Duplex {
            return new Duplex(
                \Pam\Async\connect($address, $timeout, $tls),
                $highWaterMark,
            );
        }

        public static function pipe(
            ReadableStream $source,
            WritableStream $destination,
            ?float $timeout = null,
        ): int {
            $written = 0;
            while (!$source->eof()) {
                $chunk = $source->read(timeout: $timeout);
                if ($chunk !== '') {
                    $written += $destination->write($chunk, $timeout);
                }
            }
            return $written;
        }
    }
}
