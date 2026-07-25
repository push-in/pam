<?php

declare(strict_types=1);

namespace App\Repositories;

use App\Enums\WorkspaceStatus;
use App\Models\Workspace;
use Illuminate\Database\Eloquent\Collection;

final class WorkspaceRepository
{
    /** @return Collection<int, Workspace> */
    public function latest(): Collection
    {
        return Workspace::query()->latest('id')->get();
    }

    public function create(string $name, WorkspaceStatus $status): Workspace
    {
        return Workspace::query()->create([
            'name' => $name,
            'status' => $status,
        ]);
    }
}
