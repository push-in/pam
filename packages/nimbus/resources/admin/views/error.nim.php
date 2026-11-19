@extends('layout')
@section('title', (string) $status)
@section('content')
<header class="page-head"><h1>{{ $status }}</h1></header>
<p class="lead">{{ $message }}</p>
<p><a class="button" href="{{ $admin->url() }}">{{ $t('back') }}</a></p>
@endsection
