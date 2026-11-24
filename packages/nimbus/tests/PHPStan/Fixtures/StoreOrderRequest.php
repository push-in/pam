<?php

declare(strict_types=1);

namespace Nimbus\Tests\PHPStan\Fixtures;

use Nimbus\Profiler\DebugEntryType;
use Nimbus\Validation\FormRequest;
use Nimbus\Validation\Rule;

final class StoreOrderRequest extends FormRequest
{
    public function rules(): array
    {
        return [
            'sku' => ['required', 'string'],
            'quantity' => ['required', 'integer'],
            'gift' => ['boolean'],
            'note' => ['string'],
            'tags' => ['array'],
            'status' => ['required', Rule::enum(DebugEntryType::class)],
        ];
    }
}
