<?php

declare(strict_types=1);

namespace Nimbus\Testing;

/**
 * Used by a Nimbus\Testing\TestCase: runs the pending migrations, then wraps each test in a
 * database transaction that is rolled back when the test ends.
 */
trait RefreshDatabase
{
}
