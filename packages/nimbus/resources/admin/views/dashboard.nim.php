@extends('layout')
@section('title', $t('dashboard'))
@section('content')
<header class="page-head"><h1>{{ $t('dashboard') }}</h1></header>
<ul class="cards">
@foreach ($cards as $card)
<li class="card"><a href="{{ $card['url'] }}"><span class="card-label">{{ $card['label'] }}</span><span class="card-count">{{ $t('records', ['count' => number_format($card['count'])]) }}</span></a></li>
@endforeach
</ul>
@endsection
