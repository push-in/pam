@extends('layout')
@section('title', $heading)
@section('content')
<header class="page-head">
<h1>{{ $heading }}</h1>
<a class="button ghost" href="{{ $indexUrl }}">{{ $t('cancel') }}</a>
</header>
@if ($hasErrors)
<div class="alert" role="alert">{{ $t('fix_errors') }}</div>
@endif
<form class="form" method="POST" action="{{ $action }}" novalidate>
@csrf
@if ($method !== null)
@method($method)
@endif
@foreach ($inputs as $input)
@include('field', $input)
@endforeach
<div class="form-actions">
<button type="submit" class="button primary">{{ $t('save') }}</button>
<a class="button ghost" href="{{ $indexUrl }}">{{ $t('cancel') }}</a>
</div>
</form>
@if ($deleteUrl !== null)
<form class="danger-zone" method="POST" action="{{ $deleteUrl }}" data-confirm="{{ $t('delete_confirm', ['title' => $title]) }}">
@csrf
@method('DELETE')
<button type="submit" class="button danger">{{ $t('delete') }}</button>
</form>
@endif
@endsection
