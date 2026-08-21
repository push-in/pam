<?php

declare(strict_types=1);

namespace Pam\Api\Config;

final class ConfigurationException extends \RuntimeException
{
    /** @param list<string> $errors */
    public function __construct(public readonly array $errors)
    {
        parent::__construct('Application configuration is invalid: ' . implode('; ', $errors));
    }
}
