<?php

declare(strict_types=1);

namespace Nimbus\Database\Pgsql;

/** Carries the server SQLSTATE the way PDO does, so Arcano maps it to the same exceptions. */
final class PgsqlException extends \PDOException
{
    public static function from(string $sqlState, string $message): self
    {
        $error = new self("SQLSTATE[{$sqlState}]: {$message}");
        $error->errorInfo = [$sqlState, null, $message];
        $error->code = $sqlState;
        return $error;
    }
}
