<?php

declare(strict_types=1);

namespace Pam\Api\Tests;

use Pam\Api\Jobs\JobCodec;
use Pam\Api\Jobs\SerializableJob;
use PHPUnit\Framework\TestCase;

final class JobCodecTest extends TestCase
{
    public function testRegisteredJobRoundTripsThroughVersionedJson(): void
    {
        $codec = new JobCodec([SerializableInvoiceJob::class]);
        $encoded = $codec->encode(new SerializableInvoiceJob(42));

        self::assertSame(
            '{"schema":1,"name":"billing.send-invoice","payload":{"invoiceId":42}}',
            $encoded,
        );
        self::assertEquals(new SerializableInvoiceJob(42), $codec->decode($encoded));
    }

    public function testUnknownTypesFailClosedBeforeHydration(): void
    {
        $codec = new JobCodec([SerializableInvoiceJob::class]);

        $this->expectException(\UnexpectedValueException::class);
        $codec->decode('{"schema":1,"name":"system.shell","payload":{}}');
    }

    public function testExtraEnvelopeFieldsAreRejected(): void
    {
        $codec = new JobCodec([SerializableInvoiceJob::class]);

        $this->expectException(\UnexpectedValueException::class);
        $codec->decode('{"schema":1,"name":"billing.send-invoice","payload":{},"class":"Unsafe"}');
    }
}

final readonly class SerializableInvoiceJob implements SerializableJob
{
    public function __construct(public int $invoiceId)
    {
    }

    public static function jobName(): string
    {
        return 'billing.send-invoice';
    }

    public function toJobPayload(): array
    {
        return ['invoiceId' => $this->invoiceId];
    }

    public static function fromJobPayload(array $payload): self
    {
        $invoiceId = $payload['invoiceId'] ?? null;
        if (!is_int($invoiceId) || $invoiceId < 1) {
            throw new \UnexpectedValueException('Invoice job payload is invalid.');
        }
        return new self($invoiceId);
    }
}
