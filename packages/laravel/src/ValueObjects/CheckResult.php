<?php

declare(strict_types=1);

namespace Pam\Laravel\ValueObjects;

use Pam\Laravel\Enums\CheckStatus;

final readonly class CheckResult
{
    public function __construct(
        public string $id,
        public CheckStatus $status,
        public string $message,
        public ?string $remediation = null,
    ) {
    }

    /** @return array{id: string, status: int, label: string, message: string, remediation: ?string} */
    public function toArray(): array
    {
        return [
            'id' => $this->id,
            'status' => $this->status->value,
            'label' => $this->status->label(),
            'message' => $this->message,
            'remediation' => $this->remediation,
        ];
    }
}
