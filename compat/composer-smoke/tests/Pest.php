<?php

declare(strict_types=1);

use Pam\Compatibility\Probe;

test('Composer packages are loadable from Pest', function (): void {
    expect(Probe::packages())
        ->amp->toBe(42)
        ->guzzle->toBeTrue()
        ->react->toBeTrue();
});
