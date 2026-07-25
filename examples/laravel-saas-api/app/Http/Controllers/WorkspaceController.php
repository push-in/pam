<?php

declare(strict_types=1);

namespace App\Http\Controllers;

use App\Http\Requests\StoreWorkspaceRequest;
use App\Http\Resources\WorkspaceResource;
use App\Repositories\WorkspaceRepository;
use App\Services\CreateWorkspace;
use Illuminate\Http\JsonResponse;
use Illuminate\Http\Resources\Json\AnonymousResourceCollection;
use Symfony\Component\HttpFoundation\Response;

final class WorkspaceController
{
    public function index(WorkspaceRepository $workspaces): AnonymousResourceCollection
    {
        return WorkspaceResource::collection($workspaces->latest());
    }

    public function store(StoreWorkspaceRequest $request, CreateWorkspace $create): JsonResponse
    {
        $workspace = $create->handle($request->string('name')->toString());

        return (new WorkspaceResource($workspace))
            ->response()
            ->setStatusCode(Response::HTTP_CREATED);
    }
}
