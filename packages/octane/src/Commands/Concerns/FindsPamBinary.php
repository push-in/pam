<?php

declare(strict_types=1);

namespace Pam\Octane\Commands\Concerns;

use Symfony\Component\Process\ExecutableFinder;

trait FindsPamBinary
{
    /**
     * The PAM binary: PAM_BINARY (set by PAM for `pam artisan ...`), the
     * PATH, the project root or ~/.local/bin (where install.sh puts it).
     */
    protected function findPamBinary(): ?string
    {
        $binary = getenv('PAM_BINARY');
        if (is_string($binary) && $binary !== '' && is_file($binary) && is_executable($binary)) {
            return $binary;
        }

        $home = $_SERVER['HOME'] ?? $_ENV['HOME'] ?? getenv('HOME');

        return (new ExecutableFinder())->find('pam', null, array_values(array_filter([
            base_path(),
            is_string($home) && $home !== '' ? $home.'/.local/bin' : null,
        ])));
    }
}
