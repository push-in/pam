<?php

declare(strict_types=1);

namespace Tests\Feature;

use Illuminate\Foundation\Testing\RefreshDatabase;
use Tests\TestCase;

final class WorkspaceApiTest extends TestCase
{
    use RefreshDatabase;

    public function test_it_creates_and_lists_a_trial_workspace(): void
    {
        $this->postJson('/api/workspaces', ['name' => 'Community'])
            ->assertCreated()
            ->assertJsonPath('data.name', 'Community')
            ->assertJsonPath('data.status', 1);

        $this->getJson('/api/workspaces')
            ->assertOk()
            ->assertJsonCount(1, 'data');
    }
}
