<?php

declare(strict_types=1);

namespace Nimbus\Transactions;

interface TransactionManager
{
    public function transaction(callable $operation): mixed;
}

