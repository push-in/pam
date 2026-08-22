<?php

declare(strict_types=1);

namespace Pam\Contracts\Transport;

final readonly class TransportDescriptor
{
    public const int PROTOCOL_VERSION = 1;

    /** @var list<TransportCapability> */
    public array $capabilities;

    /** @param list<mixed> $capabilities */
    public function __construct(
        public string $id,
        public TransportKind $kind,
        array $capabilities,
        public int $maxPayloadBytes = 1_048_576,
        public int $maxBatchSize = 100,
        public int $protocolVersion = self::PROTOCOL_VERSION,
    ) {
        if (preg_match('/^[a-z][a-z0-9]*(?:[.-][a-z0-9]+)*$/D', $id) !== 1 || strlen($id) > 96) {
            throw new \InvalidArgumentException('Transport ID must be a bounded lowercase dotted identifier.');
        }
        if ($protocolVersion !== self::PROTOCOL_VERSION) {
            throw new \InvalidArgumentException('Transport protocol version is incompatible with this core API.');
        }
        if ($maxPayloadBytes < 1 || $maxPayloadBytes > 16_777_216) {
            throw new \InvalidArgumentException('Transport payload limit must be between 1 byte and 16 MiB.');
        }
        if ($maxBatchSize < 1 || $maxBatchSize > 1_000) {
            throw new \InvalidArgumentException('Transport batch size must be between 1 and 1,000.');
        }
        if ($capabilities === []) {
            throw new \InvalidArgumentException('Transport must declare at least one capability.');
        }
        $validated = [];
        $values = [];
        foreach ($capabilities as $capability) {
            if (!$capability instanceof TransportCapability || isset($values[$capability->value])) {
                throw new \InvalidArgumentException('Transport capabilities must be unique enum values.');
            }
            $values[$capability->value] = true;
            $validated[] = $capability;
        }
        $this->capabilities = $validated;
    }

    public function supports(TransportCapability $capability): bool
    {
        return in_array($capability, $this->capabilities, true);
    }
}
