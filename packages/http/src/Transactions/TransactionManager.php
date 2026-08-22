<?php

declare(strict_types=1);

namespace Pam\Api\Transactions;

interface TransactionManager
{
    public function transaction(callable $operation): mixed;
}

