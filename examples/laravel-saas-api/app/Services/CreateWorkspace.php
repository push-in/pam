<?php

declare(strict_types=1);

namespace App\Services;

use App\Enums\WorkspaceStatus;
use App\Models\Workspace;
use App\Repositories\WorkspaceRepository;

final readonly class CreateWorkspace
{
    public function __construct(private WorkspaceRepository $workspaces)
    {
    }

    public function handle(string $name): Workspace
    {
        return $this->workspaces->create($name, WorkspaceStatus::Trial);
    }
}
