@extends('layout')
@section('title', $resource->label())
@section('content')
<header class="page-head">
<h1>{{ $resource->label() }}</h1>
<p class="muted">{{ $t('results', ['total' => number_format((int) $meta['total'])]) }}</p>
@if ($createUrl !== null)
<a class="button primary" href="{{ $createUrl }}">{{ $t('new', ['resource' => $resource->singularLabel()]) }}</a>
@endif
</header>
@if ($searchable || $filters !== [])
<form class="toolbar" method="GET" action="{{ $indexUrl }}" role="search">
@if ($searchable)
<label class="field-inline"><span class="sr-only">{{ $t('search') }}</span><input type="search" name="q" value="{{ $search }}" placeholder="{{ $t('search') }}"></label>
@endif
@foreach ($filters as $filter)
<label class="field-inline"><span>{{ $filter['label'] }}</span>
<select name="f[{{ $filter['name'] }}]">
<option value="">{{ $t('all') }}</option>
@foreach ($filter['options'] as $value => $label)
<option value="{{ $value }}" @selected((string) $value === $filter['value'])>{{ $label }}</option>
@endforeach
</select></label>
@endforeach
<button type="submit" class="button">{{ $t('filter') }}</button>
@if ($search !== '' || array_filter(array_column($filters, 'value')) !== [])
<a class="button ghost" href="{{ $indexUrl }}">{{ $t('clear') }}</a>
@endif
</form>
@endif
<form method="POST" action="{{ $bulkUrl }}" id="bulk">
@csrf
<input type="hidden" name="_back" value="{{ $back }}">
<div class="table-wrap">
<table>
<caption class="sr-only">{{ $resource->label() }}</caption>
<thead>
<tr>
@if ($actions !== [])
<th scope="col" class="check"><input type="checkbox" data-select-all aria-label="{{ $t('select_all') }}"></th>
@endif
@foreach ($columns as $column)
<th scope="col" class="type-{{ $column['type'] }}" @if ($column['sorted'] !== null) aria-sort="{{ $column['sorted'] === 'asc' ? 'ascending' : 'descending' }}" @endif>
@if ($column['url'] !== null)
<a href="{{ $column['url'] }}">{{ $column['label'] }}<span class="sort" aria-hidden="true">{{ $column['sorted'] === 'asc' ? '▲' : ($column['sorted'] === 'desc' ? '▼' : '') }}</span></a>
@else
{{ $column['label'] }}
@endif
</th>
@endforeach
<th scope="col" class="row-actions"><span class="sr-only">{{ $t('actions') }}</span></th>
</tr>
</thead>
<tbody>
@forelse ($rows as $row)
<tr>
@if ($actions !== [])
<td class="check"><input type="checkbox" name="ids[]" value="{{ $row['id'] }}" aria-label="{{ $t('select_row', ['title' => $row['title']]) }}"></td>
@endif
@foreach ($row['cells'] as $cell)
<td>{{ $cell }}</td>
@endforeach
<td class="row-actions">
@if ($row['edit'] !== null)
<a href="{{ $row['edit'] }}">{{ $t('edit_link') }}<span class="sr-only"> {{ $row['title'] }}</span></a>
@endif
</td>
</tr>
@empty
<tr><td colspan="{{ count($columns) + ($actions !== [] ? 2 : 1) }}" class="empty">{{ $t('empty') }}</td></tr>
@endforelse
</tbody>
</table>
</div>
@if ($actions !== [] && $rows !== [])
<div class="bulk">
<label class="field-inline"><span>{{ $t('with_selected') }}</span>
<select name="action">
@foreach ($actions as $name => $label)
<option value="{{ $name }}"@if ($name === 'delete') data-confirm="{{ $t('bulk_delete_confirm') }}"@endif>{{ $label }}</option>
@endforeach
</select></label>
<button type="submit" class="button">{{ $t('apply') }}</button>
</div>
@endif
</form>
@if ((int) $meta['last_page'] > 1)
<nav class="pagination" aria-label="{{ $t('page', ['current' => $meta['current_page'], 'last' => $meta['last_page']]) }}">
@if ($links['prev'] !== null)<a class="button" href="{{ $links['prev'] }}" rel="prev">{{ $t('previous') }}</a>@else<span class="button" aria-disabled="true">{{ $t('previous') }}</span>@endif
<span>{{ $t('page', ['current' => $meta['current_page'], 'last' => $meta['last_page']]) }}</span>
@if ($links['next'] !== null)<a class="button" href="{{ $links['next'] }}" rel="next">{{ $t('next') }}</a>@else<span class="button" aria-disabled="true">{{ $t('next') }}</span>@endif
</nav>
@endif
@endsection
